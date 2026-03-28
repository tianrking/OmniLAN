# OmniLAN

`OmniLAN` 是一个用 Rust 实现的新一代局域网代理网关编排器。  
目标是做一个更现代、可扩展、单机高性能的 LAN Gateway 控制层，而不是重复实现代理内核本身。

仓库地址：[https://github.com/tianrking/OmniLAN](https://github.com/tianrking/OmniLAN)

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
omnilan doctor -c omnilan.yaml
sudo omnilan service-install -c omnilan.yaml
sudo omnilan service-uninstall
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

采用分层架构（`cli / application / domain / core / infra`）：

1. `src/cli/`
- 命令定义与参数入口

2. `src/application/`
- 命令流程编排、运行生命周期、诊断输出

3. `src/domain/`
- 配置模型与状态快照（纯数据层）

4. `src/core/engine/`
- `Engine` trait 与双内核实现（mihomo/sing-box）

5. `src/core/gateway/`
- 网关能力编排入口

6. `src/core/enforcement/`
- DHCP Assist、Policy Route、回滚脚本输出

7. `src/infra/platform/`
- 跨平台网络适配实现（Linux/macOS/Windows）

8. `src/infra/service/`
- systemd / launchd / Windows 任务计划服务管理

9. `src/infra/audit/`
- JSONL 审计日志

详细设计文档见：`docs/ARCHITECTURE.md`

## 内核与扩展

- 双核心支持：`mihomo` / `sing-box`
- 引擎适配器模式：后续扩展 Hysteria/TUIC/Xray 更直接
- 配置层统一：平台行为不散落在脚本中，便于持续演进
- 强制接入策略层：`gateway-only / dhcp-assist / policy-route`
- 跨平台架构：`platform` 适配层统一系统网络操作
- 运维诊断：`doctor` 命令输出平台能力和关键依赖状态

## CI

已内置 GitHub Actions CI：

- Ubuntu + macOS + Windows 三平台矩阵
- `fmt` / `clippy -D warnings` / `test` / `build --release`

配置文件：`.github/workflows/ci.yml`

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
