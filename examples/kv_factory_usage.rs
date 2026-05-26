// KV Store Factory Pattern Usage Example
// KV存储工厂模式使用示例

use spear_next::storage::{
    create_kv_store_from_config, create_kv_store_from_env, DefaultKvStoreFactory, KvStore,
    KvStoreConfig, KvStoreFactory,
};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("KV Store Factory Pattern Examples");
    println!("KV存储工厂模式示例");
    println!("================================");

    // Example 1: Basic factory usage
    // 示例1：基本工厂使用
    println!("\n1. Basic Factory Usage / 基本工厂使用:");
    basic_factory_usage().await?;

    // Example 2: Configuration-driven selection
    // 示例2：配置驱动选择
    println!("\n2. Configuration-driven Selection / 配置驱动选择:");
    config_driven_selection().await?;

    // Example 3: Environment variable configuration
    // 示例3：环境变量配置
    println!("\n3. Environment Variable Configuration / 环境变量配置:");
    env_var_configuration().await?;

    // Example 4: Application mode-based selection
    // 示例4：基于应用模式的选择
    println!("\n4. Application Mode-based Selection / 基于应用模式的选择:");
    app_mode_selection().await?;

    // Example 5: Runtime backend switching
    // 示例5：运行时后端切换
    println!("\n5. Runtime Backend Switching / 运行时后端切换:");
    runtime_backend_switching().await?;

    Ok(())
}

// Basic factory usage example
// 基本工厂使用示例
async fn basic_factory_usage() -> Result<(), Box<dyn std::error::Error>> {
    let factory = DefaultKvStoreFactory::new();

    // Check supported backends / 检查支持的后端
    let backends = factory.supported_backends();
    println!("Supported backends / 支持的后端: {:?}", backends);

    // Create memory store / 创建内存存储
    let config = KvStoreConfig::memory();
    factory.validate_config(&config)?;
    let store = factory.create(&config).await?;

    // Test basic operations / 测试基本操作
    let key = "test_key".to_string();
    let value = "test_value".as_bytes().to_vec();

    store.put(&key, &value).await?;
    let retrieved = store.get(&key).await?;
    println!(
        "Retrieved value / 检索到的值: {:?}",
        String::from_utf8_lossy(&retrieved.unwrap())
    );

    Ok(())
}

// Configuration-driven selection example
// 配置驱动选择示例
async fn config_driven_selection() -> Result<(), Box<dyn std::error::Error>> {
    // Memory configuration with parameters / 带参数的内存配置
    let memory_config = KvStoreConfig::memory()
        .with_param("cache_size", "5000")
        .with_param("debug", "true");

    let _memory_store = create_kv_store_from_config(&memory_config).await?;
    println!("Created memory store with config / 使用配置创建内存存储");

    // Sled configuration (if feature enabled) / Sled配置（如果启用特性）
    #[cfg(feature = "sled")]
    {
        let sled_config =
            KvStoreConfig::sled("/tmp/test_db").with_param("cache_capacity", "100000");

        let _sled_store = create_kv_store_from_config(&sled_config).await?;
        println!("Created sled store with config / 使用配置创建sled存储");
    }

    Ok(())
}

// Environment variable configuration example
// 环境变量配置示例
async fn env_var_configuration() -> Result<(), Box<dyn std::error::Error>> {
    // Set environment variables / 设置环境变量
    env::set_var("KV_STORE_BACKEND", "memory");
    env::set_var("KV_STORE_CACHE_SIZE", "3000");
    env::set_var("KV_STORE_DEBUG", "true");

    // Create store from environment / 从环境变量创建存储
    let store = create_kv_store_from_env().await?;
    println!("Created store from environment variables / 从环境变量创建存储");

    // Test the store / 测试存储
    let key = "env_test".to_string();
    let value = "environment configured".as_bytes().to_vec();

    store.put(&key, &value).await?;
    let exists = store.exists(&key).await?;
    println!("Key exists / 键存在: {}", exists);

    // Clean up / 清理
    env::remove_var("KV_STORE_BACKEND");
    env::remove_var("KV_STORE_CACHE_SIZE");
    env::remove_var("KV_STORE_DEBUG");

    Ok(())
}

// Application mode-based selection example
// 基于应用模式的选择示例
async fn app_mode_selection() -> Result<(), Box<dyn std::error::Error>> {
    let modes = vec!["test", "dev", "prod"];

    for mode in modes {
        let store = create_store_for_mode(mode).await?;
        println!(
            "Created store for mode '{}' / 为模式'{}'创建存储",
            mode, mode
        );

        // Test the store / 测试存储
        let key = format!("{}_key", mode);
        let value = format!("{}_value", mode).as_bytes().to_vec();

        store.put(&key, &value).await?;
        let count = store.count().await?;
        println!("Store has {} items / 存储有{}个项目", count, count);
    }

    Ok(())
}

// Helper function for mode-based store creation
// 基于模式创建存储的辅助函数
async fn create_store_for_mode(mode: &str) -> Result<Box<dyn KvStore>, Box<dyn std::error::Error>> {
    let config = match mode {
        "test" => KvStoreConfig::memory(),
        "dev" => KvStoreConfig::memory().with_param("debug", "true"),
        #[cfg(feature = "sled")]
        "prod" => KvStoreConfig::sled(&format!("/tmp/prod_db_{}", std::process::id()))
            .with_param("cache_capacity", "100000"),
        #[cfg(not(feature = "sled"))]
        "prod" => KvStoreConfig::memory().with_param("cache_size", "10000"),
        _ => return Err(format!("Unsupported mode: {}", mode).into()),
    };

    Ok(create_kv_store_from_config(&config).await?)
}

// Runtime backend switching example
// 运行时后端切换示例
async fn runtime_backend_switching() -> Result<(), Box<dyn std::error::Error>> {
    // Simulate different runtime conditions / 模拟不同的运行时条件
    let conditions = vec![
        ("TESTING", "true"),
        ("DATABASE_PATH", "/tmp/runtime_db"),
        ("", ""), // Default case / 默认情况
    ];

    for (env_key, env_value) in conditions {
        if !env_key.is_empty() {
            env::set_var(env_key, env_value);
        }

        let store = create_store_based_on_conditions().await?;
        println!(
            "Created store for condition '{}={}' / 为条件'{}'='{}'创建存储",
            env_key, env_value, env_key, env_value
        );

        // Test the store / 测试存储
        let key = "runtime_test".to_string();
        let value = format!("condition_{}", env_key).as_bytes().to_vec();

        store.put(&key, &value).await?;
        let retrieved = store.get(&key).await?;
        if let Some(value) = retrieved {
            let value_str = String::from_utf8_lossy(&value);
            println!("Retrieved: {} / 检索到: {}", value_str, value_str);
        }

        if !env_key.is_empty() {
            env::remove_var(env_key);
        }
    }

    Ok(())
}

// Helper function for condition-based store creation
// 基于条件创建存储的辅助函数
async fn create_store_based_on_conditions() -> Result<Box<dyn KvStore>, Box<dyn std::error::Error>>
{
    let config = if env::var("TESTING").is_ok() {
        KvStoreConfig::memory().with_param("test_mode", "true")
    } else if let Ok(_db_path) = env::var("DATABASE_PATH") {
        #[cfg(feature = "sled")]
        {
            KvStoreConfig::sled(db_path)
        }
        #[cfg(not(feature = "sled"))]
        {
            KvStoreConfig::memory().with_param("simulated_persistence", "true")
        }
    } else {
        KvStoreConfig::memory().with_param("default", "true")
    };

    Ok(create_kv_store_from_config(&config).await?)
}
