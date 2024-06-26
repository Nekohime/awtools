use std::{env, net::Ipv4Addr, path::PathBuf};

use super::configurator::run_configurator;
use aw_db::{DatabaseConfig, DatabaseType, MysqlConfig, SqliteConfig};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Struct representing all configurations in the config file.
#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    pub universe: UniverseConfig,
    pub sql: DatabaseConfig,
}

/// Configuration section for the universe
#[derive(Deserialize, Serialize, Debug)]
pub struct UniverseConfig {
    pub license_ip: Ipv4Addr,
    pub bind_ip: Ipv4Addr,
    pub port: u16,
    pub user_list: bool,
    pub allow_citizen_changes: bool,
    pub allow_immigration: bool,
    pub connection_limit: u16,
    pub player_limit: u16,
}

impl Config {
    /// Read and (if necessary) generate configuation file.
    pub fn get_interactive(config_path: impl AsRef<Path>) -> Result<Self, String> {
        // Check if config file exists. If not, run configurator.
        // If it does exist, parse it.
        let config_path = config_path.as_ref();

        let config = if !config_path.exists() {
            println!(
                "No config file was found at {}. Running configurator.",
                config_path.display()
            );
            run_configurator()
        } else {
            match std::fs::read_to_string(config_path) {
                Ok(contents) => toml::from_str(&contents).map_err(|e| e.to_string())?,
                Err(why) => Err(why.to_string())?,
            }
        };

        config.save(config_path);

        Ok(config)
    }

    /// Write configuation to disk.
    pub fn save(&self, config_path: impl AsRef<Path>) {
        let contents = toml::to_string(&self).unwrap_or_default();
        std::fs::write(config_path, contents).ok();
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            universe: UniverseConfig {
                license_ip: Ipv4Addr::new(127, 0, 0, 1),
                bind_ip: Ipv4Addr::new(0, 0, 0, 0),
                port: 6670,
                user_list: true,
                allow_citizen_changes: true,
                allow_immigration: true,
                connection_limit: 200,
                player_limit: 100,
            },
            sql: DatabaseConfig {
                database_type: DatabaseType::Internal,
                mysql_config: MysqlConfig {
                    hostname: "127.0.0.1".to_string(),
                    port: 3306,
                    username: "root".to_string(),
                    password: "password".to_string(),
                    database: "aworld_universe".to_string(),
                },
                sqlite_config: SqliteConfig {
                    path: {
                        // The default path should be "universe.db" in the current directory
                        let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                        let default_path = current_dir.join("universe.db");
                        let path_str = default_path.to_str().unwrap_or("universe.db").to_string();
                        path_str
                    },
                },
            },
        }
    }
}
