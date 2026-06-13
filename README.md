# cxc

## 中文

`cxc` 是一个给 mirror API 用户用的小工具，当前先服务这类场景：快速切换 Claude Code 和 Codex 的认证配置。

它的思路很直接：不用每次手动进 `vim`、`nano` 之类编辑配置文件，也不用再装 `ccswitch` 这类额外切换软件。你只需要在终端里输入新的 key 和 base URL，`cxc` 就会把对应字段改到本机配置文件里。

当前支持：
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

### 适用场景

- 你在用 mirror API
- 你经常切换不同 key 或不同代理地址
- 你只想快速改配置，不想每次手动打开配置文件

### 构建

```bash
cargo build --release
```

编译完成后，二进制在：

```bash
target/release/cxc
```

可以直接放到 `~/bin`：

```bash
cp target/release/cxc ~/bin/
chmod +x ~/bin/cxc
```

### 用法

切换 Claude Code 配置：

```bash
cxc --cc
```

切换 Codex 配置：

```bash
cxc --cx
```

所有输入都是明文输入，方便直接键入或粘贴。

### 安全性

- 只替换指定字段
- 写入使用临时文件替换，避免半写入
- 写入后会重新读取并校验目标值确实已经生效

### 验证

```bash
cargo fmt --check
cargo test
cargo build --release
```

## English

`cxc` is a small CLI for people currently using mirror APIs to switch Claude Code and Codex credentials quickly.

The idea is simple: instead of opening config files in `vim` or `nano`, or relying on extra switching tools like `ccswitch`, you type a new key and base URL in the terminal and `cxc` updates the right fields for you.

Supported flows:
- `cxc --cc`
  - updates `~/.claude/settings.json`
  - replaces `env.ANTHROPIC_AUTH_TOKEN`
  - replaces `env.ANTHROPIC_BASE_URL`
- `cxc --cx`
  - updates `~/.codex/config.toml`
  - replaces `[model_providers.mirror].base_url`
  - updates `~/.codex/auth.json`
  - replaces `OPENAI_API_KEY`

If a target file is missing, malformed, or the target key does not exist, the tool exits with an error. It does not silently add fields or modify unrelated content.

### Good fit when

- you are using a mirror API
- you switch keys or proxy endpoints often
- you want a fast config change without editing files manually

### Build

```bash
cargo build --release
```

The binary will be available at:

```bash
target/release/cxc
```

You can move it into `~/bin`:

```bash
cp target/release/cxc ~/bin/
chmod +x ~/bin/cxc
```

### Usage

Switch Claude Code config:

```bash
cxc --cc
```

Switch Codex config:

```bash
cxc --cx
```

All inputs are plain text, so typing or pasting works directly.

### Safety

- only replaces the target fields
- writes through a temporary file to avoid partial updates
- reads the file back and verifies the new value after writing

### Verify

```bash
cargo fmt --check
cargo test
cargo build --release
```
