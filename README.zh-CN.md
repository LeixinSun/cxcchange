# cxc

`cxc` 是一个给 mirror API 用户用的小工具，用来快速切换 Claude Code 和 Codex 的认证配置。

它的目标很直接：在终端里直接改 key 和 API 地址，不用每次手动进 `vim`、`nano` 改配置文件，也不用依赖 `ccswitch` 这种额外切换软件。

English README: [README.md](./README.md)

## 会改哪些内容

- `cxc --cc`
  - 修改 `~/.claude/settings.json`
  - 替换 `env.ANTHROPIC_AUTH_TOKEN`
  - 替换 `env.ANTHROPIC_BASE_URL`
- `cxc --cx`
  - 修改 `~/.codex/config.toml`
  - 替换 `[model_providers.mirror].base_url`
  - 修改 `~/.codex/auth.json`
  - 替换 `OPENAI_API_KEY`

如果目标文件不存在、格式不合法、或者目标 key 不存在，程序会直接报错退出，不会偷偷补字段，也不会改别的内容。

## 适用场景

- 你在用 mirror API
- 你经常切换不同 key 或不同代理地址
- 你只想快速改配置，不想每次手动打开配置文件

## 构建

```bash
cargo build --release
```

编译完成后，二进制在：

```bash
target/release/cxc
```

macOS / Linux 可以直接放到 `~/bin`，Windows 可以放到任意已经加入 `PATH` 的目录。

## 用法

切换 Claude Code 配置：

```bash
cxc --cc
```

切换 Codex 配置：

```bash
cxc --cx
```

查看当前正在使用的配置：

```bash
cxc current
```

保存一个可复用配置到 `~/.cxc/profiles/<name>.toml`：

```bash
cxc save work
```

执行时会先问你这是给 Claude Code (`cc`) 还是 Codex (`cx`) 用的，然后自动保存成 `cc-work` 或 `cx-work`。

直接使用一个已保存配置：

```bash
cxc use cc-work
```

如果你只执行：

```bash
cxc use
```

程序会先问你要用 `cc` 还是 `cx`，再把这一类已保存配置列出来让你选。

列出已保存配置：

```bash
cxc list
```

所有输入都是明文输入，方便直接键入或粘贴。

## 平台支持

- macOS / Linux 使用标准家目录路径
- Windows 使用当前用户家目录，并通过 Windows 文件替换 API 覆盖配置文件

## 安全性

- 只替换指定字段
- 写入时先落到同目录临时文件
- 写入后会重新读取并校验目标值确实已经生效

## 验证

```bash
cargo fmt --check
cargo test
cargo build --release
```
