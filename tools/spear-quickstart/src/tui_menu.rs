use std::sync::Arc;

use crate::config::Config;

use super::{
    port_forward_summary, port_forward_summary_console, port_forward_summary_web_admin,
    AccessorAppString, AccessorBool, AccessorString, Action, App, ItemKind, MenuItem, MenuScreen,
    ScopePart,
};

pub(super) fn refresh_current_values(app: &mut App) {
    for si in 0..app.stack.len() {
        let kinds: Vec<ItemKind> = app.stack[si]
            .items
            .iter()
            .map(|it| it.kind.clone())
            .collect();
        for (ii, kind) in kinds.into_iter().enumerate() {
            let v = render_value(app, &kind);
            app.stack[si].items[ii].value = v;
        }
    }
}

fn render_value(app: &App, kind: &ItemKind) -> String {
    match kind {
        ItemKind::ToggleBool(acc) => {
            if (acc.get)(&app.cfg) {
                "[*]".to_string()
            } else {
                "[ ]".to_string()
            }
        }
        ItemKind::ToggleScope(part) => {
            if scope_contains(&app.cleanup_scope, part.key()) {
                "[*]".to_string()
            } else {
                "[ ]".to_string()
            }
        }
        ItemKind::EditString(acc) => (acc.get)(&app.cfg),
        ItemKind::EditAppString(acc) => (acc.get)(app),
        ItemKind::Submenu(screen) => {
            if screen.title.starts_with("Cleanup scope") {
                app.cleanup_scope.to_string()
            } else if screen.title.starts_with("Mode") {
                app.cfg.mode.name.clone()
            } else if screen.title.starts_with("Port forward (web admin)") {
                port_forward_summary_web_admin(app)
            } else if screen.title.starts_with("Port forward (console)") {
                port_forward_summary_console(app)
            } else if screen.title.starts_with("Port forward") {
                port_forward_summary(app)
            } else if screen.title.starts_with("Docker local") {
                format!(
                    "{}  {}:{}  admin:{}  {}:{}",
                    app.cfg.docker_local.network_name,
                    app.cfg.docker_local.sms_name,
                    app.cfg.docker_local.publish_sms_http,
                    app.cfg.docker_local.publish_sms_web_admin,
                    app.cfg.docker_local.spearlet_name,
                    app.cfg.docker_local.publish_spearlet_http
                )
            } else if screen.title.starts_with("Debian suite") {
                app.cfg.build.debian_suite.clone()
            } else if screen.title.starts_with("Log level") {
                app.cfg.logging.log_level.clone()
            } else if screen.title.starts_with("Log format") {
                app.cfg.logging.log_format.clone()
            } else if screen.title.starts_with("Rollout timeout") {
                app.cfg.timeouts.rollout.clone()
            } else {
                "".to_string()
            }
        }
        ItemKind::Action(_) => "".to_string(),
    }
}

struct MenuItemSpec {
    label: &'static str,
    kind: ItemKind,
    visible_when: Option<fn(&Config) -> bool>,
}

fn vis_mode_k8s_kind(cfg: &Config) -> bool {
    cfg.mode.name == "k8s-kind"
}

fn vis_mode_k8s_existing(cfg: &Config) -> bool {
    cfg.mode.name == "k8s-existing"
}

fn vis_mode_docker_local(cfg: &Config) -> bool {
    cfg.mode.name == "docker-local"
}

fn vis_mode_k8s_any(cfg: &Config) -> bool {
    cfg.mode.name == "k8s-kind" || cfg.mode.name == "k8s-existing"
}

fn items_from_specs(cfg: &Config, specs: Vec<MenuItemSpec>) -> Vec<MenuItem> {
    specs
        .into_iter()
        .filter(|s| s.visible_when.map(|f| f(cfg)).unwrap_or(true))
        .map(|s| MenuItem {
            label: s.label.to_string(),
            value: "".to_string(),
            kind: s.kind,
        })
        .collect()
}

pub(super) fn build_root_menu(cfg: &Config) -> MenuScreen {
    let specs = vec![
        MenuItemSpec {
            label: "Mode / 模式",
            kind: ItemKind::Submenu(build_mode_menu()),
            visible_when: None,
        },
        MenuItemSpec {
            label: "Cleanup scope / 清理范围",
            kind: ItemKind::Submenu(build_cleanup_scope_menu(cfg)),
            visible_when: None,
        },
        MenuItemSpec {
            label: "Port forward / 端口转发",
            kind: ItemKind::Submenu(build_port_forward_menu()),
            visible_when: Some(vis_mode_k8s_kind),
        },
        MenuItemSpec {
            label: "Docker local / 本地Docker",
            kind: ItemKind::Submenu(build_docker_local_menu()),
            visible_when: Some(vis_mode_docker_local),
        },
        MenuItemSpec {
            label: "K8s / Kubernetes",
            kind: ItemKind::Submenu(build_k8s_menu(cfg)),
            visible_when: Some(vis_mode_k8s_any),
        },
        MenuItemSpec {
            label: "Build / 构建",
            kind: ItemKind::Submenu(build_build_menu()),
            visible_when: None,
        },
        MenuItemSpec {
            label: "Images / 镜像",
            kind: ItemKind::Submenu(build_images_menu()),
            visible_when: None,
        },
        MenuItemSpec {
            label: "Components / 组件",
            kind: ItemKind::Submenu(build_components_menu()),
            visible_when: None,
        },
        MenuItemSpec {
            label: "Logging / 日志",
            kind: ItemKind::Submenu(build_logging_menu()),
            visible_when: None,
        },
        MenuItemSpec {
            label: "Secrets / 密钥",
            kind: ItemKind::Submenu(build_secrets_menu(cfg)),
            visible_when: None,
        },
    ];
    let items = items_from_specs(cfg, specs);

    MenuScreen {
        title: "SPEAR Quickstart".to_string(),
        items,
        selected: 0,
        visible_when: None,
    }
}

fn build_docker_local_menu() -> MenuScreen {
    MenuScreen {
        title: "Docker local / 本地Docker".to_string(),
        items: vec![
            MenuItem {
                label: "Network / 网络".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.docker_local.network_name.clone()),
                    set: Arc::new(|c, v| c.docker_local.network_name = v),
                }),
            },
            MenuItem {
                label: "SMS container name / SMS容器名".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.docker_local.sms_name.clone()),
                    set: Arc::new(|c, v| c.docker_local.sms_name = v),
                }),
            },
            MenuItem {
                label: "Spearlet container name / Spearlet容器名".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.docker_local.spearlet_name.clone()),
                    set: Arc::new(|c, v| c.docker_local.spearlet_name = v),
                }),
            },
            MenuItem {
                label: "Publish SMS HTTP (Console) / 映射SMS HTTP（控制台）".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.docker_local.publish_sms_http.clone()),
                    set: Arc::new(|c, v| c.docker_local.publish_sms_http = v),
                }),
            },
            MenuItem {
                label: "Publish SMS Web Admin / 映射SMS管理页".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.docker_local.publish_sms_web_admin.clone()),
                    set: Arc::new(|c, v| c.docker_local.publish_sms_web_admin = v),
                }),
            },
            MenuItem {
                label: "Publish Spearlet HTTP / 映射Spearlet HTTP".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.docker_local.publish_spearlet_http.clone()),
                    set: Arc::new(|c, v| c.docker_local.publish_spearlet_http = v),
                }),
            },
            MenuItem {
                label: "Back / 返回".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::Back),
            },
        ],
        selected: 0,
        visible_when: Some(vis_mode_docker_local),
    }
}

fn build_port_forward_menu() -> MenuScreen {
    MenuScreen {
        title: "Port forward / 端口转发".to_string(),
        items: vec![
            MenuItem {
                label: "SMS Web Admin / 管理页".to_string(),
                value: "".to_string(),
                kind: ItemKind::Submenu(build_port_forward_web_admin_menu()),
            },
            MenuItem {
                label: "SPEAR Console / 控制台".to_string(),
                value: "".to_string(),
                kind: ItemKind::Submenu(build_port_forward_console_menu()),
            },
            MenuItem {
                label: "Back / 返回".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::Back),
            },
        ],
        selected: 0,
        visible_when: Some(vis_mode_k8s_kind),
    }
}

fn build_port_forward_web_admin_menu() -> MenuScreen {
    MenuScreen {
        title: "Port forward (web admin) / 端口转发（管理页）".to_string(),
        items: vec![
            MenuItem {
                label: "Enabled / 启用".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(AccessorBool {
                    get: Arc::new(|c| c.k8s.port_forward.enabled),
                    set: Arc::new(|c, v| c.k8s.port_forward.enabled = v),
                }),
            },
            MenuItem {
                label: "Auto start after apply / 部署后自动启动".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(AccessorBool {
                    get: Arc::new(|c| c.k8s.port_forward.auto_start),
                    set: Arc::new(|c, v| c.k8s.port_forward.auto_start = v),
                }),
            },
            MenuItem {
                label: "Local port / 本地端口".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.k8s.port_forward.local_port.to_string()),
                    set: Arc::new(|c, v| {
                        if let Ok(p) = v.trim().parse::<u16>() {
                            if p != 0 {
                                c.k8s.port_forward.local_port = p;
                            }
                        }
                    }),
                }),
            },
            MenuItem {
                label: "Remote port / 远端端口".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.k8s.port_forward.remote_port.to_string()),
                    set: Arc::new(|c, v| {
                        if let Ok(p) = v.trim().parse::<u16>() {
                            if p != 0 {
                                c.k8s.port_forward.remote_port = p;
                            }
                        }
                    }),
                }),
            },
            MenuItem {
                label: "Start / 启动".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::StartPortForwardWebAdmin),
            },
            MenuItem {
                label: "Stop / 停止".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::StopPortForwardWebAdmin),
            },
            MenuItem {
                label: "Back / 返回".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::Back),
            },
        ],
        selected: 0,
        visible_when: Some(vis_mode_k8s_kind),
    }
}

fn build_port_forward_console_menu() -> MenuScreen {
    MenuScreen {
        title: "Port forward (console) / 端口转发（控制台）".to_string(),
        items: vec![
            MenuItem {
                label: "Enabled / 启用".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(AccessorBool {
                    get: Arc::new(|c| c.k8s.port_forward_console.enabled),
                    set: Arc::new(|c, v| c.k8s.port_forward_console.enabled = v),
                }),
            },
            MenuItem {
                label: "Auto start after apply / 部署后自动启动".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(AccessorBool {
                    get: Arc::new(|c| c.k8s.port_forward_console.auto_start),
                    set: Arc::new(|c, v| c.k8s.port_forward_console.auto_start = v),
                }),
            },
            MenuItem {
                label: "Local port / 本地端口".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.k8s.port_forward_console.local_port.to_string()),
                    set: Arc::new(|c, v| {
                        if let Ok(p) = v.trim().parse::<u16>() {
                            if p != 0 {
                                c.k8s.port_forward_console.local_port = p;
                            }
                        }
                    }),
                }),
            },
            MenuItem {
                label: "Remote port / 远端端口".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.k8s.port_forward_console.remote_port.to_string()),
                    set: Arc::new(|c, v| {
                        if let Ok(p) = v.trim().parse::<u16>() {
                            if p != 0 {
                                c.k8s.port_forward_console.remote_port = p;
                            }
                        }
                    }),
                }),
            },
            MenuItem {
                label: "Start / 启动".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::StartPortForwardConsole),
            },
            MenuItem {
                label: "Stop / 停止".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::StopPortForwardConsole),
            },
            MenuItem {
                label: "Back / 返回".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::Back),
            },
        ],
        selected: 0,
        visible_when: Some(vis_mode_k8s_kind),
    }
}

fn build_cleanup_scope_menu(cfg: &Config) -> MenuScreen {
    let specs = vec![
        MenuItemSpec {
            label: "release (helm uninstall)",
            kind: ItemKind::ToggleScope(ScopePart::Release),
            visible_when: Some(vis_mode_k8s_any),
        },
        MenuItemSpec {
            label: "containers (docker rm)",
            kind: ItemKind::ToggleScope(ScopePart::Release),
            visible_when: Some(vis_mode_docker_local),
        },
        MenuItemSpec {
            label: "secret (k8s secret)",
            kind: ItemKind::ToggleScope(ScopePart::Secret),
            visible_when: Some(vis_mode_k8s_any),
        },
        MenuItemSpec {
            label: "namespace (DANGEROUS)",
            kind: ItemKind::ToggleScope(ScopePart::Namespace),
            visible_when: Some(vis_mode_k8s_any),
        },
        MenuItemSpec {
            label: "kind (DANGEROUS)",
            kind: ItemKind::ToggleScope(ScopePart::Kind),
            visible_when: Some(vis_mode_k8s_kind),
        },
        MenuItemSpec {
            label: "network (DANGEROUS)",
            kind: ItemKind::ToggleScope(ScopePart::Kind),
            visible_when: Some(vis_mode_docker_local),
        },
        MenuItemSpec {
            label: "images (docker rmi) (DANGEROUS)",
            kind: ItemKind::ToggleScope(ScopePart::Images),
            visible_when: Some(vis_mode_docker_local),
        },
        MenuItemSpec {
            label: "Raw csv / 原始csv",
            kind: ItemKind::EditAppString(AccessorAppString {
                get: Arc::new(|a| a.cleanup_scope.clone()),
                set: Arc::new(|a, v| a.cleanup_scope = v),
            }),
            visible_when: None,
        },
        MenuItemSpec {
            label: "Back / 返回",
            kind: ItemKind::Action(Action::Back),
            visible_when: None,
        },
    ];
    let items = items_from_specs(cfg, specs);

    MenuScreen {
        title: "Cleanup scope / 清理范围".to_string(),
        items,
        selected: 0,
        visible_when: None,
    }
}

fn build_mode_menu() -> MenuScreen {
    let mut items = vec![];

    let set_mode = |mode: &'static str| -> AccessorBool {
        let get = move |c: &Config| c.mode.name == mode;
        let set = move |c: &mut Config, v: bool| {
            if v {
                c.mode.name = mode.to_string();
            }
        };
        AccessorBool {
            get: Arc::new(get),
            set: Arc::new(set),
        }
    };

    items.push(MenuItem {
        label: "k8s-kind".to_string(),
        value: "".to_string(),
        kind: ItemKind::ToggleBool(set_mode("k8s-kind")),
    });
    items.push(MenuItem {
        label: "k8s-existing".to_string(),
        value: "".to_string(),
        kind: ItemKind::ToggleBool(set_mode("k8s-existing")),
    });
    items.push(MenuItem {
        label: "docker-local".to_string(),
        value: "".to_string(),
        kind: ItemKind::ToggleBool(set_mode("docker-local")),
    });

    items.push(MenuItem {
        label: "Back / 返回".to_string(),
        value: "".to_string(),
        kind: ItemKind::Action(Action::Back),
    });

    MenuScreen {
        title: "Mode / 模式".to_string(),
        items,
        selected: 0,
        visible_when: None,
    }
}

fn build_k8s_menu(cfg: &Config) -> MenuScreen {
    let specs = vec![
        MenuItemSpec {
            label: "Namespace / 命名空间",
            kind: ItemKind::EditString(AccessorString {
                get: Arc::new(|c| c.k8s.namespace.clone()),
                set: Arc::new(|c, v| c.k8s.namespace = v),
            }),
            visible_when: None,
        },
        MenuItemSpec {
            label: "Release / 发布名",
            kind: ItemKind::EditString(AccessorString {
                get: Arc::new(|c| c.k8s.release_name.clone()),
                set: Arc::new(|c, v| c.k8s.release_name = v),
            }),
            visible_when: None,
        },
        MenuItemSpec {
            label: "Kind cluster name / kind 集群名",
            kind: ItemKind::EditString(AccessorString {
                get: Arc::new(|c| c.k8s.kind.cluster_name.clone()),
                set: Arc::new(|c, v| c.k8s.kind.cluster_name = v),
            }),
            visible_when: Some(vis_mode_k8s_kind),
        },
        MenuItemSpec {
            label: "Reuse existing kind cluster / 复用已有 kind 集群",
            kind: ItemKind::ToggleBool(AccessorBool {
                get: Arc::new(|c| c.k8s.kind.reuse_cluster),
                set: Arc::new(|c, v| c.k8s.kind.reuse_cluster = v),
            }),
            visible_when: Some(vis_mode_k8s_kind),
        },
        MenuItemSpec {
            label: "Keep kind cluster after run / 运行结束保留 kind 集群",
            kind: ItemKind::ToggleBool(AccessorBool {
                get: Arc::new(|c| c.k8s.kind.keep_cluster),
                set: Arc::new(|c, v| c.k8s.kind.keep_cluster = v),
            }),
            visible_when: Some(vis_mode_k8s_kind),
        },
        MenuItemSpec {
            label: "Kind kubeconfig file / kind kubeconfig 文件",
            kind: ItemKind::EditString(AccessorString {
                get: Arc::new(|c| c.k8s.kind.kubeconfig_file.clone()),
                set: Arc::new(|c, v| c.k8s.kind.kubeconfig_file = v),
            }),
            visible_when: Some(vis_mode_k8s_kind),
        },
        MenuItemSpec {
            label: "Existing kubeconfig / 现有集群kubeconfig",
            kind: ItemKind::EditString(AccessorString {
                get: Arc::new(|c| c.k8s.existing.kubeconfig.clone()),
                set: Arc::new(|c, v| c.k8s.existing.kubeconfig = v),
            }),
            visible_when: Some(vis_mode_k8s_existing),
        },
        MenuItemSpec {
            label: "Existing context / 现有集群context",
            kind: ItemKind::EditString(AccessorString {
                get: Arc::new(|c| c.k8s.existing.context.clone()),
                set: Arc::new(|c, v| c.k8s.existing.context = v),
            }),
            visible_when: Some(vis_mode_k8s_existing),
        },
        MenuItemSpec {
            label: "Back / 返回",
            kind: ItemKind::Action(Action::Back),
            visible_when: None,
        },
    ];
    let items = items_from_specs(cfg, specs);

    MenuScreen {
        title: "K8s / Kubernetes".to_string(),
        items,
        selected: 0,
        visible_when: Some(vis_mode_k8s_any),
    }
}

fn build_build_menu() -> MenuScreen {
    MenuScreen {
        title: "Build / 构建".to_string(),
        items: vec![
            MenuItem {
                label: "Enabled / 启用".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(AccessorBool {
                    get: Arc::new(|c| c.build.enabled),
                    set: Arc::new(|c, v| c.build.enabled = v),
                }),
            },
            MenuItem {
                label: "Pull base / 拉取基础镜像".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(AccessorBool {
                    get: Arc::new(|c| c.build.pull_base),
                    set: Arc::new(|c, v| c.build.pull_base = v),
                }),
            },
            MenuItem {
                label: "No cache / 禁用缓存".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(AccessorBool {
                    get: Arc::new(|c| c.build.no_cache),
                    set: Arc::new(|c, v| c.build.no_cache = v),
                }),
            },
            MenuItem {
                label: "Debian suite / Debian版本".to_string(),
                value: "".to_string(),
                kind: ItemKind::Submenu(build_debian_suite_menu()),
            },
            MenuItem {
                label: "Back / 返回".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::Back),
            },
        ],
        selected: 0,
        visible_when: None,
    }
}

fn build_debian_suite_menu() -> MenuScreen {
    let set_suite = |suite: &'static str| -> AccessorBool {
        let get = move |c: &Config| c.build.debian_suite.eq_ignore_ascii_case(suite);
        let set = move |c: &mut Config, v: bool| {
            if v {
                c.build.debian_suite = suite.to_string();
            }
        };
        AccessorBool {
            get: Arc::new(get),
            set: Arc::new(set),
        }
    };

    MenuScreen {
        title: "Debian suite / Debian版本".to_string(),
        items: vec![
            MenuItem {
                label: "bookworm".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(set_suite("bookworm")),
            },
            MenuItem {
                label: "trixie".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(set_suite("trixie")),
            },
            MenuItem {
                label: "Custom / 自定义".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.build.debian_suite.clone()),
                    set: Arc::new(|c, v| c.build.debian_suite = v),
                }),
            },
            MenuItem {
                label: "Back / 返回".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::Back),
            },
        ],
        selected: 0,
        visible_when: None,
    }
}

fn build_images_menu() -> MenuScreen {
    MenuScreen {
        title: "Images / 镜像".to_string(),
        items: vec![
            MenuItem {
                label: "Image tag / 镜像tag".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.images.tag.clone()),
                    set: Arc::new(|c, v| c.images.tag = v),
                }),
            },
            MenuItem {
                label: "Unified repo / 统一repo（可选）".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.images.unified_repo.clone()),
                    set: Arc::new(|c, v| c.images.unified_repo = v),
                }),
            },
            MenuItem {
                label: "SMS repo".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.images.sms_repo.clone()),
                    set: Arc::new(|c, v| c.images.sms_repo = v),
                }),
            },
            MenuItem {
                label: "Spearlet repo".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.images.spearlet_repo.clone()),
                    set: Arc::new(|c, v| c.images.spearlet_repo = v),
                }),
            },
            MenuItem {
                label: "Back / 返回".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::Back),
            },
        ],
        selected: 0,
        visible_when: None,
    }
}

fn build_components_menu() -> MenuScreen {
    MenuScreen {
        title: "Components / 组件".to_string(),
        items: vec![
            MenuItem {
                label: "Web admin / Web管理页".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(AccessorBool {
                    get: Arc::new(|c| c.components.enable_web_admin),
                    set: Arc::new(|c, v| c.components.enable_web_admin = v),
                }),
            },
            MenuItem {
                label: "Router filter / 路由过滤".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(AccessorBool {
                    get: Arc::new(|c| c.components.enable_router_filter),
                    set: Arc::new(|c, v| c.components.enable_router_filter = v),
                }),
            },
            MenuItem {
                label: "E2E / 端到端".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(AccessorBool {
                    get: Arc::new(|c| c.components.enable_e2e),
                    set: Arc::new(|c, v| c.components.enable_e2e = v),
                }),
            },
            MenuItem {
                label: "Spearlet with Node / Spearlet含Node".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(AccessorBool {
                    get: Arc::new(|c| c.components.spearlet_with_node),
                    set: Arc::new(|c, v| c.components.spearlet_with_node = v),
                }),
            },
            MenuItem {
                label: "Llama server / Llama服务".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(AccessorBool {
                    get: Arc::new(|c| c.components.spearlet_with_llama_server),
                    set: Arc::new(|c, v| c.components.spearlet_with_llama_server = v),
                }),
            },
            MenuItem {
                label: "Back / 返回".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::Back),
            },
        ],
        selected: 0,
        visible_when: None,
    }
}

fn build_logging_menu() -> MenuScreen {
    MenuScreen {
        title: "Logging / 日志".to_string(),
        items: vec![
            MenuItem {
                label: "Debug / 调试模式".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(AccessorBool {
                    get: Arc::new(|c| c.logging.debug),
                    set: Arc::new(|c, v| c.logging.debug = v),
                }),
            },
            MenuItem {
                label: "Log level / 日志级别".to_string(),
                value: "".to_string(),
                kind: ItemKind::Submenu(build_log_level_menu()),
            },
            MenuItem {
                label: "Log format / 日志格式".to_string(),
                value: "".to_string(),
                kind: ItemKind::Submenu(build_log_format_menu()),
            },
            MenuItem {
                label: "Rollout timeout / 部署超时".to_string(),
                value: "".to_string(),
                kind: ItemKind::Submenu(build_rollout_timeout_menu()),
            },
            MenuItem {
                label: "Back / 返回".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::Back),
            },
        ],
        selected: 0,
        visible_when: None,
    }
}

fn build_rollout_timeout_menu() -> MenuScreen {
    let set_timeout = |timeout: &'static str| -> AccessorBool {
        let get = move |c: &Config| c.timeouts.rollout.trim() == timeout;
        let set = move |c: &mut Config, v: bool| {
            if v {
                c.timeouts.rollout = timeout.to_string();
            }
        };
        AccessorBool {
            get: Arc::new(get),
            set: Arc::new(set),
        }
    };

    MenuScreen {
        title: "Rollout timeout / 部署超时".to_string(),
        items: vec![
            MenuItem {
                label: "60s".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(set_timeout("60s")),
            },
            MenuItem {
                label: "120s".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(set_timeout("120s")),
            },
            MenuItem {
                label: "300s".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(set_timeout("300s")),
            },
            MenuItem {
                label: "600s".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(set_timeout("600s")),
            },
            MenuItem {
                label: "Custom / 自定义".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.timeouts.rollout.clone()),
                    set: Arc::new(|c, v| c.timeouts.rollout = v),
                }),
            },
            MenuItem {
                label: "Back / 返回".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::Back),
            },
        ],
        selected: 0,
        visible_when: None,
    }
}

fn build_secrets_menu(cfg: &Config) -> MenuScreen {
    let specs = vec![
        MenuItemSpec {
            label: "OpenAI source / OpenAI来源",
            kind: ItemKind::Submenu(build_openai_source_menu()),
            visible_when: None,
        },
        MenuItemSpec {
            label: "OpenAI env / OpenAI环境变量",
            kind: ItemKind::EditString(AccessorString {
                get: Arc::new(|c| c.secrets.openai.env_name.clone()),
                set: Arc::new(|c, v| c.secrets.openai.env_name = v),
            }),
            visible_when: None,
        },
        MenuItemSpec {
            label: "k8s secret name / Secret名",
            kind: ItemKind::EditString(AccessorString {
                get: Arc::new(|c| c.secrets.openai.k8s_secret_name.clone()),
                set: Arc::new(|c, v| c.secrets.openai.k8s_secret_name = v),
            }),
            visible_when: Some(vis_mode_k8s_any),
        },
        MenuItemSpec {
            label: "k8s secret key / Secret键",
            kind: ItemKind::EditString(AccessorString {
                get: Arc::new(|c| c.secrets.openai.k8s_secret_key.clone()),
                set: Arc::new(|c, v| c.secrets.openai.k8s_secret_key = v),
            }),
            visible_when: Some(vis_mode_k8s_any),
        },
        MenuItemSpec {
            label: "Back / 返回",
            kind: ItemKind::Action(Action::Back),
            visible_when: None,
        },
    ];
    let items = items_from_specs(cfg, specs);

    MenuScreen {
        title: "Secrets / 密钥".to_string(),
        items,
        selected: 0,
        visible_when: None,
    }
}

fn build_openai_source_menu() -> MenuScreen {
    let set_source = |source: &'static str| -> AccessorBool {
        let get = move |c: &Config| c.secrets.openai.source == source;
        let set = move |c: &mut Config, v: bool| {
            if v {
                c.secrets.openai.source = source.to_string();
            }
        };
        AccessorBool {
            get: Arc::new(get),
            set: Arc::new(set),
        }
    };

    let items = vec![
        MenuItem {
            label: "from-env".to_string(),
            value: "".to_string(),
            kind: ItemKind::ToggleBool(set_source("from-env")),
        },
        MenuItem {
            label: "skip".to_string(),
            value: "".to_string(),
            kind: ItemKind::ToggleBool(set_source("skip")),
        },
        MenuItem {
            label: "Back / 返回".to_string(),
            value: "".to_string(),
            kind: ItemKind::Action(Action::Back),
        },
    ];

    MenuScreen {
        title: "OpenAI secret source".to_string(),
        items,
        selected: 0,
        visible_when: None,
    }
}

fn build_log_level_menu() -> MenuScreen {
    let set_level = |level: &'static str| -> AccessorBool {
        let get = move |c: &Config| c.logging.log_level.eq_ignore_ascii_case(level);
        let set = move |c: &mut Config, v: bool| {
            if v {
                c.logging.log_level = level.to_string();
            }
        };
        AccessorBool {
            get: Arc::new(get),
            set: Arc::new(set),
        }
    };

    MenuScreen {
        title: "Log level / 日志级别".to_string(),
        items: vec![
            MenuItem {
                label: "trace".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(set_level("trace")),
            },
            MenuItem {
                label: "debug".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(set_level("debug")),
            },
            MenuItem {
                label: "info".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(set_level("info")),
            },
            MenuItem {
                label: "warn".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(set_level("warn")),
            },
            MenuItem {
                label: "error".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(set_level("error")),
            },
            MenuItem {
                label: "Custom / 自定义".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.logging.log_level.clone()),
                    set: Arc::new(|c, v| c.logging.log_level = v),
                }),
            },
            MenuItem {
                label: "Back / 返回".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::Back),
            },
        ],
        selected: 0,
        visible_when: None,
    }
}

fn build_log_format_menu() -> MenuScreen {
    let set_format = |format: &'static str| -> AccessorBool {
        let get = move |c: &Config| c.logging.log_format.eq_ignore_ascii_case(format);
        let set = move |c: &mut Config, v: bool| {
            if v {
                c.logging.log_format = format.to_string();
            }
        };
        AccessorBool {
            get: Arc::new(get),
            set: Arc::new(set),
        }
    };

    MenuScreen {
        title: "Log format / 日志格式".to_string(),
        items: vec![
            MenuItem {
                label: "json".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(set_format("json")),
            },
            MenuItem {
                label: "pretty".to_string(),
                value: "".to_string(),
                kind: ItemKind::ToggleBool(set_format("pretty")),
            },
            MenuItem {
                label: "Custom / 自定义".to_string(),
                value: "".to_string(),
                kind: ItemKind::EditString(AccessorString {
                    get: Arc::new(|c| c.logging.log_format.clone()),
                    set: Arc::new(|c, v| c.logging.log_format = v),
                }),
            },
            MenuItem {
                label: "Back / 返回".to_string(),
                value: "".to_string(),
                kind: ItemKind::Action(Action::Back),
            },
        ],
        selected: 0,
        visible_when: None,
    }
}

pub(super) fn scope_contains(scope: &str, key: &str) -> bool {
    scope
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .any(|s| s == key)
}

pub(super) fn scope_set(scope: &mut String, key: &str, enabled: bool) {
    let mut parts: Vec<String> = scope
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    parts.retain(|p| p != key);
    if enabled {
        parts.push(key.to_string());
    }

    let order = ["release", "secret", "namespace", "kind", "images"];
    parts.sort_by_key(|p| order.iter().position(|x| *x == p.as_str()).unwrap_or(99));
    parts.dedup();

    if parts.is_empty() {
        *scope = "release".to_string();
    } else {
        *scope = parts.join(",");
    }
}
