// Copyright 2019-2021 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use cargo_mobile::{
  android::{
    adb,
    config::{Config as AndroidConfig, Metadata as AndroidMetadata},
    device::{Device, RunError},
    env::{Env, Error as AndroidEnvError},
    target::{BuildError, Target},
  },
  config::Config,
  device::PromptError,
  env::Error as EnvError,
  os,
  util::prompt,
};
use clap::{Parser, Subcommand};

use super::{
  ensure_init, get_config,
  init::{command as init_command, init_dot_cargo, Options as InitOptions},
  log_finished, Target as MobileTarget,
};
use crate::{helpers::config::get as get_tauri_config, Result};

mod android_studio_script;
mod build;
mod dev;
mod open;
pub(crate) mod project;

#[derive(Debug, thiserror::Error)]
enum Error {
  #[error(transparent)]
  EnvInitFailed(EnvError),
  #[error(transparent)]
  AndroidEnvInitFailed(AndroidEnvError),
  #[error(transparent)]
  InitDotCargo(super::init::Error),
  #[error("invalid tauri configuration: {0}")]
  InvalidTauriConfig(String),
  #[error("{0}")]
  ProjectNotInitialized(String),
  #[error(transparent)]
  OpenFailed(os::OpenFileError),
  #[error("{0}")]
  DevFailed(String),
  #[error("{0}")]
  BuildFailed(String),
  #[error(transparent)]
  AndroidStudioScriptFailed(BuildError),
  #[error(transparent)]
  RunFailed(RunError),
  #[error("{0}")]
  TargetInvalid(String),
  #[error(transparent)]
  FailedToPromptForDevice(PromptError<adb::device_list::Error>),
}

#[derive(Parser)]
#[clap(
  author,
  version,
  about = "Android commands",
  subcommand_required(true),
  arg_required_else_help(true)
)]
pub struct Cli {
  #[clap(subcommand)]
  command: Commands,
}

#[derive(Subcommand)]
enum Commands {
  Init(InitOptions),
  /// Open project in Android Studio
  Open,
  Dev(dev::Options),
  Build(build::Options),
  #[clap(hide(true))]
  AndroidStudioScript(android_studio_script::Options),
}

pub fn command(cli: Cli) -> Result<()> {
  match cli.command {
    Commands::Init(options) => init_command(options, MobileTarget::Android)?,
    Commands::Open => open::command()?,
    Commands::Dev(options) => dev::command(options)?,
    Commands::Build(options) => build::command(options)?,
    Commands::AndroidStudioScript(options) => android_studio_script::command(options)?,
  }

  Ok(())
}

fn with_config<T>(
  f: impl FnOnce(&Config, &AndroidConfig, &AndroidMetadata) -> Result<T, Error>,
) -> Result<T, Error> {
  let (config, metadata) = {
    let tauri_config =
      get_tauri_config(None).map_err(|e| Error::InvalidTauriConfig(e.to_string()))?;
    let tauri_config_guard = tauri_config.lock().unwrap();
    let tauri_config_ = tauri_config_guard.as_ref().unwrap();
    get_config(tauri_config_)
  };
  f(&config, config.android(), metadata.android())
}

fn env() -> Result<Env, Error> {
  let env = super::env().map_err(Error::EnvInitFailed)?;
  cargo_mobile::android::env::Env::from_env(env).map_err(Error::AndroidEnvInitFailed)
}

fn device_prompt<'a>(env: &'_ Env) -> Result<Device<'a>, PromptError<adb::device_list::Error>> {
  let device_list =
    adb::device_list(env).map_err(|cause| PromptError::detection_failed("Android", cause))?;
  if !device_list.is_empty() {
    let index = if device_list.len() > 1 {
      prompt::list(
        concat!("Detected ", "Android", " devices"),
        device_list.iter(),
        "device",
        None,
        "Device",
      )
      .map_err(|cause| PromptError::prompt_failed("Android", cause))?
    } else {
      0
    };
    let device = device_list.into_iter().nth(index).unwrap();
    println!(
      "Detected connected device: {} with target {:?}",
      device,
      device.target().triple,
    );
    Ok(device)
  } else {
    Err(PromptError::none_detected("Android"))
  }
}

fn detect_target_ok<'a>(env: &Env) -> Option<&'a Target<'a>> {
  device_prompt(env).map(|device| device.target()).ok()
}