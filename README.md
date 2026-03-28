# OmniLAN

`OmniLAN` 是一个用 Rust 实现的新一代局域网代理网关编排器。  
目标是做一个更现代、可扩展、单机高性能的 LAN Gateway 控制层，而不是重复实现代理内核本身。

## 设计目标

- 多引擎：统一入口管理 `mihomo` 和 `sing-box`
- 统一配置：一份 YAML 配置，按引擎渲染目标配置
- 网关自动化：IP 转发/NAT 自动编排
- 单机全网覆盖：一台电脑即可承载局域网代理出口
- 可扩展：设备策略、安全策略、审计状态统一抽象
- 工程化：强类型配置、可验证、可渲染、可运行、可观测（状态文件）

## 当前命令

```bash
omnilan init
omnilan validate -c omnilan.yaml
omnilan render -c omnilan.yaml
sudo omnilan run -c omnilan.yaml
omnilan stop -c omnilan.yaml
omnilan status -c omnilan.yaml
omnilan audit -c omnilan.yaml
sudo omnilan rollback -c omnilan.yaml
```

## 快速开始

```bash
cd /Users/w0x7ce/Downloads/OMG——PROXY/OK_proxy
cargo build --release
./target/release/omnilan init
cp omnilan.example.yaml omnilan.yaml
# 编辑 omnilan.yaml，填入 subscription_url 或 local_file
sudo ./target/release/omnilan run -c omnilan.yaml
```

运行后，把需要接入的局域网设备网关和 DNS 指向这台电脑，即可无客户端使用代理。

## 架构

1. `src/config.rs`
- 配置模型、默认配置、校验逻辑

2. `src/engine/*`
- `Engine` trait：统一 `ensure_binary / prepare / command`
- `mihomo.rs`：生成 `mihomo.generated.yaml`
- `singbox.rs`：生成 `sing-box.generated.json`

3. `src/gateway/mod.rs`
- 系统网关能力编排：IP forwarding + NAT + DNS 劫持转发（Linux）

4. `src/enforcement.rs`
- `dhcp-assist`：生成 dnsmasq 可用配置（可选自动部署）
- `policy-route`：按设备 IP/MAC 强制透明重定向到代理端口
- 自动输出 rollback 脚本，支持一键撤销

5. `src/app.rs`
- CLI 命令生命周期：init/validate/render/run/stop/status/audit/rollback
- 进程管理、信号退出、状态写入

6. `src/state.rs`
- 运行状态快照（PID、启动时间、渲染文件、rollback、audit）

7. `src/audit.rs`
- 审计日志（JSONL），记录策略应用、启动、退出、回滚等事件

## 内核与扩展

- 双核心支持：`mihomo` / `sing-box`
- 引擎适配器模式：后续扩展 Hysteria/TUIC/Xray 更直接
- 配置层统一：平台行为不散落在脚本中，便于持续演进
- 强制接入策略层：`gateway-only / dhcp-assist / policy-route`

## 强制无感接入能力

- DHCP Assist：针对目标设备自动生成网关/DNS 下发规则
- Policy Route：针对目标设备 `IP/MAC` 做透明代理重定向
- 透明转发：`TUN + NAT + DNS` 组合，设备无需安装客户端
- 回滚与审计：自动生成 `rollback.sh` + `audit.log`

## 安全说明

如需做“指定设备无感接入”，建议在授权网络中使用以下现代方式：

- 网关/DHCP 静态分配 + 指定网关策略
- 防火墙白名单/路由策略
- 明确审计日志和可回滚控制

这样更可控、可审计，也更适合长期稳定运行。
