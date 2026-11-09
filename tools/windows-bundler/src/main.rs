//! Package an already compiled Windows binary with Tauri's own NSIS template.
//! No source upload, keychain access or application-data access is performed.
use std::{path::PathBuf,fs};
use tauri_bundler::{BundleBinary,BundleSettings,NsisSettings,PackageSettings,PackageType,SettingsBuilder,WindowsSettings};
fn main()->Result<(),Box<dyn std::error::Error>>{
 env_logger::Builder::from_default_env().filter_level(log::LevelFilter::Info).init();
 let mut args=std::env::args().skip(1);
 let root=PathBuf::from(args.next().ok_or("Expected repository root")?).canonicalize()?;
 let output=PathBuf::from(args.next().ok_or("Expected Windows release directory")?).canonicalize()?;
 let config:serde_json::Value=serde_json::from_slice(&fs::read(root.join("src-tauri/tauri.conf.json"))?)?;
 let field=|name:&str|->Result<String,Box<dyn std::error::Error>>{Ok(config[name].as_str().ok_or("Missing app metadata")?.into())};
 if !output.join("kouros.exe").is_file(){return Err("Compile kouros.exe before packaging".into())}
 std::env::set_current_dir(root.join("src-tauri"))?;
 let icons=config["bundle"]["icon"].as_array().map(|v|v.iter().filter_map(|s|s.as_str().map(str::to_string)).collect());
 let settings=SettingsBuilder::new()
  .project_out_directory(output)
  .local_tools_directory(root.join("../toolchains/bundler-tools"))
  .package_types(vec![PackageType::Nsis])
  .target("x86_64-pc-windows-gnu".into())
  .package_settings(PackageSettings{product_name:field("productName")?,version:field("version")?,description:"Kouros: sport and health".into(),homepage:None,authors:None,default_run:Some("kouros".into())})
  .bundle_settings(BundleSettings{identifier:Some(field("identifier")?),publisher:config["bundle"]["publisher"].as_str().map(str::to_string),icon:icons,windows:WindowsSettings{nsis:Some(NsisSettings{languages:Some(vec!["Russian".into()]),display_language_selector:false,..Default::default()}),..Default::default()},..Default::default()})
  .binaries(vec![BundleBinary::new("kouros".into(),true)])
  .build()?;
 for bundle in tauri_bundler::bundle_project(&settings)?{for path in bundle.bundle_paths{println!("{}",path.display());}}
 Ok(())
}
