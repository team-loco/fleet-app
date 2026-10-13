use std::collections::HashMap;
use std::time::Duration;

use kube::config::{AuthInfo, ExecInteractiveMode, KubeConfigOptions, Kubeconfig};

use crate::discovery::KubeContext;
use crate::error::{Error, Result, kubeconfig_reason};
use crate::shell_env::ShellEnv;

const ENV_NAME_KEY: &str = "name";
const ENV_VALUE_KEY: &str = "value";

pub fn prepare_auth(auth: &mut AuthInfo, env: &ShellEnv) {
    let Some(exec) = auth.exec.as_mut() else {
        return;
    };
    exec.interactive_mode = Some(ExecInteractiveMode::Never);
    if let Some(command) = exec.command.as_ref()
        && let Some(resolved) = env.resolve(command)
    {
        exec.command = Some(resolved.display().to_string());
    }
    let configured = exec.env.take().unwrap_or_default();
    let configured_names: Vec<&str> = configured
        .iter()
        .filter_map(|entry| entry.get(ENV_NAME_KEY).map(String::as_str))
        .collect();
    let inherited = env
        .vars
        .iter()
        .filter(|(name, _)| !configured_names.contains(&name.as_str()))
        .map(|(name, value)| {
            HashMap::from([
                (ENV_NAME_KEY.to_owned(), name.clone()),
                (ENV_VALUE_KEY.to_owned(), value.clone()),
            ])
        })
        .collect::<Vec<_>>();
    exec.env = Some(inherited.into_iter().chain(configured).collect());
}

pub async fn load_config(context: &KubeContext, env: &ShellEnv) -> Result<kube::Config> {
    let file = context.id.file.clone();
    let kubeconfig = Kubeconfig::read_from(&file).map_err(|source| Error::Kubeconfig {
        path: file.clone(),
        reason: kubeconfig_reason(&source),
    })?;
    let options = KubeConfigOptions {
        context: Some(context.id.context.clone()),
        cluster: None,
        user: None,
    };
    let mut config = kube::Config::from_custom_kubeconfig(kubeconfig, &options)
        .await
        .map_err(|source| Error::Kubeconfig {
            path: file,
            reason: kubeconfig_reason(&source),
        })?;
    prepare_auth(&mut config.auth_info, env);
    Ok(config)
}

pub async fn connect(
    context: &KubeContext,
    env: &ShellEnv,
    timeout: Duration,
) -> Result<kube::Client> {
    let mut config = load_config(context, env).await?;
    config.connect_timeout = Some(timeout);
    let build = tokio::task::spawn_blocking(move || kube::Client::try_from(config));
    let joined = tokio::time::timeout(timeout, build)
        .await
        .map_err(|_| Error::Timeout(timeout))?;
    let client = joined.map_err(|err| Error::Exec(err.to_string()))??;
    Ok(client)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use kube::config::ExecConfig;

    use super::*;
    use crate::shell_env::EnvOrigin;

    #[test]
    fn exec_gets_shell_env_and_never_prompts() {
        let mut vars = BTreeMap::new();
        vars.insert("PATH".to_owned(), "/bin:/usr/bin".to_owned());
        vars.insert("AWS_PROFILE".to_owned(), "shell".to_owned());
        let env = ShellEnv::from_vars(
            vars,
            EnvOrigin::Fallback {
                reason: String::new(),
            },
            None,
        );
        let mut auth = AuthInfo {
            exec: Some(ExecConfig {
                command: Some("sh".to_owned()),
                env: Some(vec![HashMap::from([
                    ("name".to_owned(), "AWS_PROFILE".to_owned()),
                    ("value".to_owned(), "prod".to_owned()),
                ])]),
                ..ExecConfig::default()
            }),
            ..AuthInfo::default()
        };

        prepare_auth(&mut auth, &env);

        let Some(exec) = auth.exec else {
            panic!("exec kept");
        };
        assert_eq!(exec.interactive_mode, Some(ExecInteractiveMode::Never));
        assert_eq!(
            exec.command.map(PathBuf::from),
            Some(PathBuf::from("/bin/sh"))
        );
        let entries = exec.env.unwrap_or_default();
        let profiles: Vec<&str> = entries
            .iter()
            .filter(|entry| entry.get("name").map(String::as_str) == Some("AWS_PROFILE"))
            .filter_map(|entry| entry.get("value").map(String::as_str))
            .collect();
        assert_eq!(profiles, vec!["prod"]);
        assert!(
            entries
                .iter()
                .any(|entry| entry.get("name").map(String::as_str) == Some("PATH"))
        );
    }
}
