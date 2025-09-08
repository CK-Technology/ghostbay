use anyhow::Result;
use clap::{Parser, Subcommand};
use ghostbay_auth::{AuthService, CreateAccessKeyRequest, AccessKeyRepository};
use ghostbay_catalog::{CatalogService, CreateBucketRequest, BucketRepository};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about = "GhostBay CLI - Manage your S3-compatible object storage", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, default_value = "sqlite:./ghostbay.db")]
    database_url: String,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Run {
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    Admin {
        #[command(subcommand)]
        command: AdminCommands,
    },
    Bucket {
        #[command(subcommand)]
        command: BucketCommands,
    },
}

#[derive(Subcommand, Debug)]
enum AdminCommands {
    Key {
        #[command(subcommand)]
        command: KeyCommands,
    },
}

#[derive(Subcommand, Debug)]
enum KeyCommands {
    Create {
        #[arg(long, default_value = "admin")]
        policies: Vec<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, help = "Expiration in days from now")]
        expires_days: Option<u64>,
    },
    List {
        #[arg(long, help = "Include inactive keys")]
        include_inactive: bool,
    },
    Rotate {
        access_key_id: String,
    },
    Deactivate {
        access_key_id: String,
    },
    Delete {
        access_key_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum BucketCommands {
    Create {
        name: String,
        #[arg(long, default_value = "us-east-1")]
        region: String,
    },
    List,
    Delete {
        name: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // color_eyre::install()?;

    let cli = Cli::parse();

    match &cli.command {
        Commands::Run { config } => {
            println!("Starting GhostBay server...");
            println!("Use 'ghostbay-gateway' binary instead for running the server");
            println!("Config path: {:?}", config);
        }
        Commands::Admin { command } => {
            handle_admin_command(command, &cli.database_url).await?;
        }
        Commands::Bucket { command } => {
            handle_bucket_command(command, &cli.database_url).await?;
        }
    }

    Ok(())
}

async fn handle_admin_command(command: &AdminCommands, database_url: &str) -> Result<()> {
    match command {
        AdminCommands::Key { command } => {
            handle_key_command(command, database_url).await?;
        }
    }
    Ok(())
}

async fn handle_key_command(command: &KeyCommands, database_url: &str) -> Result<()> {
    let catalog = CatalogService::new(database_url).await?;

    // Ensure database exists and is migrated
    ghostbay_catalog::migrations::ensure_database_exists(database_url).await?;
    ghostbay_catalog::migrations::run_migrations(catalog.pool()).await?;

    let key_repo = AccessKeyRepository::new(catalog.pool().clone());

    match command {
        KeyCommands::Create { policies, description, expires_days } => {
            let expires_at = expires_days.map(|days| {
                chrono::Utc::now() + chrono::Duration::days(days as i64)
            });

            let request = CreateAccessKeyRequest {
                policies: policies.clone(),
                description: description.clone(),
                expires_at,
            };

            match key_repo.create(request).await {
                Ok(access_key) => {
                    println!("Created access key:");
                    println!("  Access Key ID: {}", access_key.access_key_id);
                    println!("  Secret Access Key: {}", access_key.secret_access_key);
                    println!("  Policies: {:?}", access_key.policies);
                    println!("  Created: {}", access_key.created_at);
                    if let Some(expires) = access_key.expires_at {
                        println!("  Expires: {}", expires);
                    }
                    if let Some(desc) = access_key.description {
                        println!("  Description: {}", desc);
                    }
                    println!("\n⚠️  Warning: Store the secret access key securely - it will not be shown again!");
                }
                Err(e) => {
                    eprintln!("Failed to create access key: {}", e);
                    std::process::exit(1);
                }
            }
        }
        KeyCommands::List { include_inactive } => {
            match key_repo.list(*include_inactive).await {
                Ok(keys) => {
                    if keys.is_empty() {
                        println!("No access keys found");
                    } else {
                        println!("Access Keys:");
                        for key in keys {
                            let status = if key.is_active { "Active" } else { "Inactive" };
                            println!("  {} - {} ({})", key.access_key_id, status, key.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
                            if let Some(expires) = key.expires_at {
                                println!("    Expires: {}", expires.format("%Y-%m-%d %H:%M:%S UTC"));
                            }
                            if let Some(desc) = key.description {
                                println!("    Description: {}", desc);
                            }
                            println!("    Policies: {:?}", key.policies);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to list access keys: {}", e);
                    std::process::exit(1);
                }
            }
        }
        KeyCommands::Rotate { access_key_id } => {
            match key_repo.rotate(access_key_id).await {
                Ok(Some(access_key)) => {
                    println!("Rotated access key:");
                    println!("  Access Key ID: {}", access_key.access_key_id);
                    println!("  New Secret Access Key: {}", access_key.secret_access_key);
                    println!("  Updated: {}", access_key.created_at);
                    println!("\n⚠️  Warning: Store the new secret access key securely - it will not be shown again!");
                }
                Ok(None) => {
                    eprintln!("Access key '{}' not found", access_key_id);
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Failed to rotate access key: {}", e);
                    std::process::exit(1);
                }
            }
        }
        KeyCommands::Deactivate { access_key_id } => {
            match key_repo.deactivate(access_key_id).await {
                Ok(true) => {
                    println!("Deactivated access key '{}'", access_key_id);
                }
                Ok(false) => {
                    eprintln!("Access key '{}' not found", access_key_id);
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Failed to deactivate access key: {}", e);
                    std::process::exit(1);
                }
            }
        }
        KeyCommands::Delete { access_key_id } => {
            match key_repo.delete(access_key_id).await {
                Ok(true) => {
                    println!("Deleted access key '{}'", access_key_id);
                }
                Ok(false) => {
                    eprintln!("Access key '{}' not found", access_key_id);
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Failed to delete access key: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
    Ok(())
}

async fn handle_bucket_command(command: &BucketCommands, database_url: &str) -> Result<()> {
    let catalog = CatalogService::new(database_url).await?;

    // Ensure database exists and is migrated
    ghostbay_catalog::migrations::ensure_database_exists(database_url).await?;
    ghostbay_catalog::migrations::run_migrations(catalog.pool()).await?;

    let repo = BucketRepository::new(catalog.pool().clone());

    match command {
        BucketCommands::Create { name, region } => {
            let request = CreateBucketRequest {
                name: name.clone(),
                region: region.clone(),
            };

            match repo.create(request).await {
                Ok(bucket) => {
                    println!("Created bucket '{}' in region '{}'", bucket.name, bucket.region);
                    println!("  ID: {}", bucket.id);
                    println!("  Created: {}", bucket.created_at);
                }
                Err(e) => {
                    eprintln!("Failed to create bucket: {}", e);
                    std::process::exit(1);
                }
            }
        }
        BucketCommands::List => {
            match repo.list().await {
                Ok(buckets) => {
                    if buckets.is_empty() {
                        println!("No buckets found");
                    } else {
                        println!("Buckets:");
                        for bucket in buckets {
                            println!("  {} ({})", bucket.name, bucket.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to list buckets: {}", e);
                    std::process::exit(1);
                }
            }
        }
        BucketCommands::Delete { name } => {
            match repo.delete(name).await {
                Ok(true) => {
                    println!("Deleted bucket '{}'", name);
                }
                Ok(false) => {
                    eprintln!("Bucket '{}' not found", name);
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Failed to delete bucket: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}