mod args;
mod config;
mod modpack_config;
mod modpack_file;

use crate::args::{KilnArgs, KilnCommand, ProjectCommand};
use crate::config::KilnConfig;
use crate::modpack_config::ModpackConfig;
use crate::modpack_file::{KilnFile, KilnMod};
use clap::Parser;
use rfd::AsyncFileDialog;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tokio::fs::{File, create_dir_all};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vintagestory_mod_db_api::VintageStoryModDbApi;
use zstd::decode_all;

#[tokio::main]
async fn main() {
    let args = KilnArgs::parse();

    if get_config().await.is_err() && args.command != KilnCommand::Setup {
        panic!("Please run `kiln setup` first`");
    }

    match args.command {
        KilnCommand::Setup => {
            if let Ok(_) = get_config().await {
                eprintln!(
                    "Config already exists! either manually edit it, or delete the file to run through initialization again"
                );
                return;
            }

            println!(
                "Select the vintage story binary (usually C:\\Users\\<username>\\Appdata\\Roaming\\Vintagestory\\Vintagestory.exe on windows, or /home/<username>/.local/share/Vintagestory/Vintagestory"
            );
            if let Some(file) = AsyncFileDialog::new()
                .set_title("Select the vintage story binary")
                .pick_file()
                .await
            {
                let path = file.path();

                let config = KilnConfig {
                    vintage_story: path.display().to_string(),
                };

                if let Err(e) = set_config(config).await {
                    eprintln!("Error setting vintage story binary: {}", e);
                    return;
                }

                println!("Successfully initialized vintage story!");
            } else {
                eprintln!("No file selected for the vintage story binary, try running again");
            }
        }
        KilnCommand::New { name } => {
            let path = get_mods_dir().join(&name);
            if path.exists() {
                eprintln!("Modpack {name} already exists!");
                return;
            }

            if let Err(e) = create_dir_all(&path).await {
                eprintln!("Error creating mods directory: {}", e);
                return;
            }

            let mut config = ModpackConfig::default();
            config.name = name.clone();
            let contents = serde_json::ser::to_string(&config)
                .expect("Could not serialize the modpack config!");
            let mut file = File::create(path.join("kiln.json"))
                .await
                .expect("Could not create the modpack config!");
            file.write_all(contents.as_bytes())
                .await
                .expect("Could not write to the modpack config!");

            println!(
                "Successfully created modpack {name} at {}",
                path.join("kiln.json").display()
            );
        }
        KilnCommand::Import { filename } => {
            let compressed =
                std::fs::File::open(&filename).expect("Could not open the compressed file!");
            let decomp = decode_all(compressed).expect("Could not decode compressed file!");
            let info: KilnFile = rmp_serde::decode::from_slice(decomp.as_slice())
                .expect("Could not decode compressed file!");

            let vs_mod_api = Arc::new(VintageStoryModDbApi::new(false));

            let location = get_mods_dir().join(&info.name);

            create_dir_all(&location.join("Mods"))
                .await
                .expect("Could not create the modpack directory!");

            let mut config = ModpackConfig::default();
            config.name = info.name;

            let mods = futures::future::join_all(
                info.mods
                    .clone()
                    .into_iter()
                    .map(async |m: KilnMod| m)
                    .collect::<Vec<_>>(),
            )
            .await;

            let download_tasks: Vec<_> = mods
                .into_iter()
                .map(|mod_info| {
                    tokio::spawn({
                        let vs_mod_api = vs_mod_api.clone();
                        let location = location.clone();
                        async move {
                            match mod_info {
                                KilnMod::ModDbMod { id, version } => {
                                    download_mod_with_version(
                                        &vs_mod_api,
                                        id.to_string(),
                                        version,
                                        &location,
                                    )
                                    .await
                                }
                                KilnMod::OtherMod { name, source } => {
                                    download_file_to(&source, &location.join(format!("{name}.zip")))
                                        .await
                                }
                            }
                        }
                    })
                })
                .collect();

            let mut values = Vec::new();

            for task in download_tasks {
                values.push(task.await.unwrap());
            }

            config.mods = info.mods;

            let contents = serde_json::ser::to_string(&config)
                .expect("Could not serialize the modpack config!");
            let mut file = File::create(&location.join("kiln.json"))
                .await
                .expect("Could not create the modpack config!");
            file.write_all(contents.as_bytes())
                .await
                .expect("Could not write to the modpack config!");
        }
        KilnCommand::Project(project_command) => match project_command {
            ProjectCommand::Add { name, id } => {
                check_modpack(&name).await;
                let mod_api = VintageStoryModDbApi::new(false);
                let mut config = read_modpack_config(&name)
                    .await
                    .expect("Could not read the modpack config!");
                for mod_info in config.mods.clone() {
                    match mod_info {
                        KilnMod::ModDbMod { id: mod_id, .. } => {
                            if mod_id == id {
                                println!("Modpack already contains id {id}, skipping");
                                return;
                            }
                        }
                        KilnMod::OtherMod { name, .. } => {
                            if name == id {
                                println!("Modpack already contains {name}, skipping");
                                return;
                            }
                        }
                    }
                }
                let version = download_mod(&mod_api, &id, get_mods_dir().join(&name).join("Mods"))
                    .await
                    .expect("Could not download a mod!");
                config.mods.push(KilnMod::ModDbMod {
                    id: id.clone(),
                    version,
                });
                write_modpack_config(name, config)
                    .await
                    .expect("Could not write the modpack config!");
                println!("Successfully added mod with id/alias {id}");
            }
            ProjectCommand::Remove { name, id } => {
                check_modpack(&name).await;

                let mut config = read_modpack_config(&name)
                    .await
                    .expect("Could not read the modpack config!");
                let filtered = config
                    .mods
                    .iter()
                    .filter(|kiln_mod| match kiln_mod {
                        KilnMod::ModDbMod { id: mod_id, .. } => *mod_id == id,
                        KilnMod::OtherMod { name, .. } => *name == id,
                    })
                    .collect::<Vec<_>>();
                if filtered.is_empty() {
                    eprintln!("No id found for {id}");
                    return;
                }
                if filtered.len() > 1 {
                    eprintln!("More than one mod found with id {id}, exiting");
                    return;
                }
                config.mods.remove(
                    config
                        .mods
                        .iter()
                        .position(|kiln_mod| kiln_mod == filtered[0])
                        .unwrap(),
                );
                write_modpack_config(name, config)
                    .await
                    .expect("Could not write the modpack config!");
            }
            ProjectCommand::Run { name } => {
                check_modpack(&name).await;

                let config = get_config().await.expect("Error getting config");

                Command::new(&config.vintage_story)
                    .arg("--dataPath")
                    .arg(get_mods_dir().join(name))
                    .spawn()
                    .expect("Failed to launch vintage story!");
            }
            ProjectCommand::Export { name } => {
                check_modpack(&name).await;

                let mut file = File::open(get_mods_dir().join(&name).join("kiln.json"))
                    .await
                    .expect("Could not open the modpack config!");
                let mut contents = String::new();
                file.read_to_string(&mut contents)
                    .await
                    .expect("Could not read the modpack config!");

                let info: KilnFile = serde_json::de::from_str(&contents)
                    .expect("Could not deserialize the modpack config!");
                let msg_pack =
                    rmp_serde::encode::to_vec(&info).expect("Could not encode the modpack config!");
                let compressed = zstd::encode_all(msg_pack.as_slice(), 3)
                    .expect("Error compressing the modpack config!");

                let output_path = format!("{name}.kiln");
                let output_path = Path::new(&output_path);
                let mut file = File::create(&output_path)
                    .await
                    .expect("Could not create the modpack config!");
                file.write_all(&compressed[..])
                    .await
                    .expect("Could not write to the modpack config!");

                println!("Successfully exported to {}", output_path.display());
            }
        },
    }
}

async fn download_mod(
    mod_api: &VintageStoryModDbApi,
    mod_id: impl AsRef<str>,
    location: impl AsRef<Path>,
) -> anyhow::Result<String> {
    let release = mod_api.get_most_recent_release_from_alias(mod_id).await?;
    let file_location = location.as_ref().join(release.get_filename());

    println!(
        "Downloading {} to {}",
        release
            .mod_id_str
            .unwrap_or("<No mod id provided in release>".to_string()),
        file_location.display()
    );

    download_file_to(&release.main_file, &file_location).await?;

    Ok(release.mod_version)
}

// TODO: move the getting specific version to vintagestory_mod_db_api
async fn download_mod_with_version(
    vintage_story_mod_db_api: &VintageStoryModDbApi,
    mod_id: impl AsRef<str>,
    version: impl AsRef<str>,
    location: impl AsRef<Path>,
) -> anyhow::Result<String> {
    let mod_info = vintage_story_mod_db_api.get_mod_from_alias(mod_id).await?;
    let mod_name = mod_info.name.clone();
    println!("Downloading {} for version {}", mod_name, version.as_ref());
    let mut releases = mod_info.releases;
    releases.retain(|release_info| release_info.mod_version == version.as_ref());
    if releases.is_empty() {
        return Err(anyhow::anyhow!(
            "No version {} found for {}",
            version.as_ref(),
            mod_name
        ));
    }
    if releases.len() > 1 {
        println!(
            "Multiple releases found for {} with version {}, using first found",
            mod_name,
            version.as_ref()
        );
    }

    let release = releases[0].clone();
    let file_location = location.as_ref().join(release.get_filename());

    download_file_to(&release.main_file, &file_location).await?;

    Ok(file_location.display().to_string())
}

async fn download_file_to(
    server_location: impl reqwest::IntoUrl,
    to: impl AsRef<Path>,
) -> anyhow::Result<String> {
    let resp = reqwest::get(server_location).await?;
    let binding = resp.bytes().await?;
    let contents = binding.as_ref();
    let mut file = File::create(&to).await?;
    file.write_all(contents).await?;
    Ok(to.as_ref().display().to_string())
}

fn get_data_dir() -> PathBuf {
    directories_next::BaseDirs::new()
        .unwrap()
        .data_dir()
        .join("Kiln")
}

fn get_mods_dir() -> PathBuf {
    get_data_dir().join("instances")
}

async fn get_config() -> anyhow::Result<KilnConfig> {
    let mut file = File::open(get_data_dir().join("conf.json")).await?;

    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;

    let config: KilnConfig = serde_json::from_str(&contents)?;
    Ok(config)
}

async fn set_config(config: KilnConfig) -> anyhow::Result<()> {
    let mut file = File::create(get_data_dir().join("conf.json")).await?;
    let contents = serde_json::ser::to_string_pretty(&config)?;

    file.write_all(contents.as_bytes()).await?;

    Ok(())
}

async fn check_modpack(name: impl AsRef<Path>) {
    let path = get_mods_dir().join(&name);
    if !path.exists() {
        panic!(
            "Modpack {0} not found, add it first with kiln add {0}",
            name.as_ref().display()
        );
    }
    create_dir_all(path.join("Mods"))
        .await
        .expect("Could not create the Mods folder!");
}

async fn read_modpack_config(name: impl AsRef<str>) -> anyhow::Result<ModpackConfig> {
    let mut file = File::open(get_mods_dir().join(name.as_ref()).join("kiln.json")).await?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;
    let config: ModpackConfig = serde_json::from_str(&contents)?;

    Ok(config)
}

async fn write_modpack_config(name: impl AsRef<str>, config: ModpackConfig) -> anyhow::Result<()> {
    let mut file = File::create(get_mods_dir().join(name.as_ref()).join("kiln.json")).await?;
    let contents = serde_json::ser::to_string(&config)?;

    file.write_all(contents.as_bytes()).await?;

    Ok(())
}
