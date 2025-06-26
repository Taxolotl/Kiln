mod args;
mod config;
mod modpack_config;
mod modpack_file;

use std::path::{Path, PathBuf};
use std::process::Command;
use clap::Parser;
use rfd::AsyncFileDialog;
use tokio::fs::{create_dir_all, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use vintagestory_mod_db_api::VintageStoryModDbApi;
use zstd::decode_all;
use crate::args::{KilnArgs, KilnCommand, ProjectCommand};
use crate::config::KilnConfig;
use crate::modpack_config::ModpackConfig;
use crate::modpack_file::{KilnFile, KilnMod};

#[tokio::main]
async fn main() {
	let args = KilnArgs::parse();

	match args.command {
		KilnCommand::Setup => {
			if let Ok(_) = get_config().await {
				eprintln!("Config already exists! either manually edit it, or delete the file to run through initialization again");
				return
			}

			println!("Select the vintage story binary (usually C:\\Users\\<username>\\Appdata\\Roaming\\Vintagestory\\Vintagestory.exe on windows, or /home/<username>/.local/share/Vintagestory/Vintagestory");
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

			let config = ModpackConfig::default();
			let contents = serde_json::ser::to_string(&config).expect("Could not serialize the modpack config!");
			let mut file = File::create(path.join("kiln.json")).await.expect("Could not create the modpack config!");
			file.write_all(contents.as_bytes()).await.expect("Could not write to the modpack config!");

			println!("Successfully created modpack {name}");
		}
		KilnCommand::Import { filename } => {
			let compressed = std::fs::File::open(&filename).expect("Could not open the compressed file!");
			let decomp = decode_all(compressed).expect("Could not decode compressed file!");
			let info: KilnFile = rmp_serde::decode::from_slice(decomp.as_slice()).expect("Could not decode compressed file!");
			
			let vs_mod_api = VintageStoryModDbApi::new(false);
			
			let location = get_mods_dir().join(&info.name);
			
			create_dir_all(&location.join("Mods")).await.expect("Could not create the modpack directory!");

			let mut config = ModpackConfig::default();
			config.name = info.name;
			
			// TODO: Make this download all async
			for mod_info in info.mods {
				match mod_info {
					KilnMod::ModDbMod { id, version } => {
						download_mod_with_version(&vs_mod_api, id.to_string(), version, &location).await.expect("Failed to download mod");
					}
					KilnMod::OtherMod { name, source } => {
						download_file_to(&source, &location.join(format!("{name}.zip"))).await.expect("Failed to download file");
					}
				}
			}
			
			config.mods = info.mods;

			let contents = serde_json::ser::to_string(&config).expect("Could not serialize the modpack config!");
			let mut file = File::create(&location.join("kiln.json")).await.expect("Could not create the modpack config!");
			file.write_all(contents.as_bytes()).await.expect("Could not write to the modpack config!");
		},
		KilnCommand::Project(project_command) => {
			match project_command {
				ProjectCommand::Add { name, id } => {
					check_modpack(&name);
					let mod_api = VintageStoryModDbApi::new(false);
					let version = download_mod(&mod_api, &id, get_mods_dir().join(&name).join("Mods")).await.expect("Could not download a mod!");
					let mut config = read_modpack_config(&name).await.expect("Could not read the modpack config!");
					config.mods.push(KilnMod::ModDbMod { id: id.clone(), version });
					write_modpack_config(name, config).await.expect("Could not write the modpack config!");
					println!("Successfully added mod with id/alias {id}");
				}
				ProjectCommand::Remove { name, id } => {
					check_modpack(&name);

					let mut config = read_modpack_config(&name).await.expect("Could not read the modpack config!");
					let filtered = config.mods.iter().filter(|kiln_mod: KilnMod| match kiln_mod {
						KilnMod::ModDbMod { id, .. } => {
							id == id
						}
						KilnMod::OtherMod { name, .. } => {
							name == id
						}
					}).collect::<Vec<_>>();
					if filtered.is_empty() {
						eprintln!("No id found for {id}");
						return;
					}
					if filtered.len() > 1 {
						eprintln!("More than one mod found with id {id}, exiting");
						return;
					}
					config.mods.remove(config.mods.iter().find(filtered[0]).unwrap());
					write_modpack_config(name, config).await.expect("Could not write the modpack config!");
				}
				ProjectCommand::Launch { name } => {
					check_modpack(&name);

					let config = get_config().await.expect("Error getting config");

					Command::new(&config.vintage_story).arg("--dataPath").arg(get_mods_dir().join(name)).spawn().expect("Failed to launch vintage story!");
				}
				ProjectCommand::Export { name } => {
					check_modpack(name);
					
					let mut file = File::open(get_mods_dir().join(&name).join("kiln.json")).await.expect("Could not open the modpack config!");
					let mut contents = String::new();
					file.read_to_string(&mut contents).await.expect("Could not read the modpack config!");
					
					let info: KilnFile = serde_json::de::from_str(&contents).expect("Could not deserialize the modpack config!");
					let msg_pack = rmp_serde::encode::to_vec(&info).expect("Could not encode the modpack config!");
					let compressed = zstd::encode_all(msg_pack, 3).expect("Error compressing the modpack config!");
					
					let output_path = Path::new(format!("{name}.kiln"));
					let mut file = File::create(output_path).await.expect("Could not create the modpack config!");
					file.write_all(&compressed[..]).await.expect("Could not write to the modpack config!");
					
					println!("Successfully exported to {}", output_path.display());
				}
			}
		}
	}

	/*
	// Async download mods
		let mod_ids: HashSet<u32> = HashSet::from([
			1065,
			890,
			82,
			1505,
			604,
			395,
			1087,
			2383,
			1183,
			821,
			562,
			2488,
			2293,
			2002,
			3802,
			1841,
			3682,
			4006,
			4186,
			1778,
			1476,
			4257,
			2460,
			1894,
			306,
			2544,
			1163,
			3912,
			16,
			3905,
			363,
			61,
			3749,
			677,
			1594,
			51,
			3611,
			792,
			3315,
			2063,
			3667,
			1520,
			3756,
			1875,
			3829,
			3599,
			3846,
			2003,
			2913,
			1311,
			2012,
			2029,
			1367,
			1639,
			3903,
			4054,
			3920,
			3853,
			3103,
			3142,
			3334,
			3424,
			3048,
			2989,
			2811,
			2711,
			2130,
			2150,
			2195,
			1254,
			1438,
			1125,
			1900,
			2019,
			2066,
			3490,
			3543,
			3684,
			3748,
			3794,
			3855,
			3886,
			3928,
			3970,
			4226,
			4155,
			4095,
			4063,
			4017,
			253,
			246,
			2347,
			3747,
			2097,
			3971,
			551,
			3954,
			2383,
			4185,
			4186,
			4187,
			1036,
			973,
			4176,
			322,
			1344,
			843
		]);

		let location = get_mods_dir()
			.join("MyPack")
			.join("Mods");
		tokio::fs::create_dir_all(&location).await.unwrap();

		let mod_api = Arc::new(VintageStoryModDbApi::new(true));
		let mod_api_clone = mod_api.clone();

		let mods: Vec<DetailedMod> = futures::future::join_all(
			mod_ids.into_iter()
				.map(async |id: u32| { mod_api_clone.clone().get_mod(id).await })
				.collect::<Vec<_>>()
		).await.into_iter().filter_map(Result::ok).collect();

		let download_tasks: Vec<_> = mods.into_iter()
			.map(|m| tokio::spawn({
				let location = location.clone();
				{
					let mod_api = mod_api.clone();
					async move {
						download_mod(&mod_api, m.mod_id.clone(), m.name.clone(), location).await;
						m
					}
				}
			}))
			.collect();
		let mut detailed_mods = Vec::new();

		for task in download_tasks {
			detailed_mods.push(task.await.unwrap());
		}

		println!("finished");
	*/
}

async fn download_mod(mod_api: &VintageStoryModDbApi, mod_id: impl AsRef<str>, location: impl AsRef<Path>) -> anyhow::Result<String> {
	let release = mod_api.get_most_recent_release_from_alias(mod_id).await?;
	let file_location = location.as_ref().join(&release.filename);

	println!("Downloading {} to {}", release.mod_id_str.unwrap_or("<No mod id provided in release>".to_string()), file_location.display());

	download_file_to(&release.main_file, &file_location).await?;

	Ok(release.mod_version)
}

// TODO: move the getting specific version to vintagestory_mod_db_api
async fn download_mod_with_version(vintage_story_mod_db_api: &VintageStoryModDbApi, mod_id: impl AsRef<str>, version: impl AsRef<str>, location: impl AsRef<Path>) -> anyhow::Result<String> {
	let mod_info = vintage_story_mod_db_api.get_mod_from_alias(mod_id).await?;
	let mod_name = mod_info.name.clone().as_str();
	println!("Downloading {mod_name} for version {version}");
	let mut releases = mod_info.releases;
	releases.retain(|release_info| release_info.mod_version == version.as_ref());
	if releases.is_empty() {
		return Err(anyhow::anyhow!("No version {version} found for {mod_name}"))
	}
	if releases.len() > 1 {
		println!("Multiple releases found for {mod_name} with version {version}, using first found");
	}
	
	let release = releases[0].clone();
	let file_location = location.as_ref().join(&release.filename);
	
	download_file_to(&release.main_file, &file_location).await?;
	
	Ok(file_location.display().to_string())
}

async fn download_file_to(server_location: impl reqwest::IntoUrl, to: impl AsRef<Path>) -> anyhow::Result<()> {
	let resp = reqwest::get(server_location).await?;
	let binding = resp.bytes().await?;
	let contents = binding.as_ref();
	let mut file = File::create(&to).await?;
	file.write_all(contents).await?;
	Ok(())
}

fn get_data_dir() -> PathBuf {
	directories_next::BaseDirs::new().unwrap().data_dir()
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

fn check_modpack(name: impl AsRef<Path>) {
	if !get_mods_dir().join(&name).exists() {
		panic!("Modpack {0} not found, add it first with kiln add {0}", name.as_ref().display());
	}
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