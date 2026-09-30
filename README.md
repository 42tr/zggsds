# 工时管理系统（zggsds）

一个轻量的内部工时管理系统：Rust 后端 + 内嵌静态前端 + SQLite，单二进制即可运行，也可用 Docker 部署。

## 功能

- **工时录入**：单条录入、按周批量录入（整周网格，自动校验工作日每日合计不低于 8 小时）
- **两级审批流**：部门负责人一审 → 项目负责人二审（交付/研发/售前类项目且设置了负责人时需要二审；审批人即项目负责人时自动跳过）
- **驳回理由**：一审/二审驳回必须填写理由，工时记录和 Excel 导出中可见
- **修改管理**：已审批工时需向部门负责人申请修改，修改记录全程留痕
- **项目管理**：编号、类型（交付/研发/售前/售后/事务）、状态、周期、负责人
- **组织管理**：部门树、用户管理、角色权限
- **注册审批**：匿名提交注册申请，自动分配给部门负责人（无负责人时分配给 admin/timekeeper）
- **统计与导出**：首页月度统计、工时记录筛选、Excel 导出

## 技术栈

| 层 | 技术 |
|---|---|
| 后端 | Rust · axum · sea-orm (SQLite) · JWT (cookie) · bcrypt |
| 前端 | 原生 HTML/CSS/JS，`rust-embed` 编译期内嵌进二进制 |
| 数据 | SQLite（`./data/time_tracking.db`，启动时自动建表与增量迁移） |

## 快速开始

### 环境变量

| 变量 | 说明 |
|---|---|
| `JWT_SECRET` | **必填**。JWT 签名密钥，建议至少 32 字节随机值 |
| `ADMIN_PASSWORD` | 首次启动初始化 admin 账号时必填，至少 12 位；已有 admin 后可不设置 |

### 本地运行

```bash
export JWT_SECRET=$(openssl rand -hex 32)
export ADMIN_PASSWORD='your-long-password'   # 仅首次初始化需要
cargo run
```

访问 http://localhost:3000 ，使用 `admin` / `ADMIN_PASSWORD` 登录。

### 测试

```bash
cargo test
```

## Docker 部署

```bash
# 构建镜像（多阶段 Dockerfile，从源码编译）
docker build -t zggsds:latest .

# 运行（数据持久化到 ./data）
export JWT_SECRET=$(openssl rand -hex 32)
export ADMIN_PASSWORD='your-long-password'
make run          # 或 docker-compose up -d
```

常用命令：`make build / run / stop / logs / rebuild / test / clean`。

推 tag 会触发 GitHub Actions：分别在 amd64/arm64 原生 Runner 编译，打包双架构镜像并推送到 GHCR 和阿里云 ACR（tag 与 `latest`）。详见 [DOCKER.md](DOCKER.md)。

## 角色与权限

| 角色 | 说明 |
|---|---|
| `employee` | 员工：录入/修改自己的工时 |
| `dept_manager` | 部门负责人：本部门工时一审、修改申请审批 |
| `project_manager` | 项目负责人：工时二审 |
| `timekeeper` | 工时管理员：查看全部工时、注册审批 |
| `admin` | 系统管理员：全部权限 |

## 项目结构

```
src/
  main.rs            路由与启动（端口 3000）
  auth.rs            JWT 签发/校验、密码哈希
  embed.rs           内嵌前端静态资源
  db/                连接与建表/增量迁移
  handlers/          auth / time_entry / project / department / user / registration*
  models/            sea-orm 实体
frontend/            index.html / login.html / register.html / app.js / style.css
```

## 说明

- 数据库文件位于 `./data/`，删除后重启会重新初始化（需再次提供 `ADMIN_PASSWORD`）
- 老库升级无需手工迁移：启动时自动补齐缺失列（忽略"已存在"错误）
- 根目录 `migration.sql` 为早期遗留参考，实际 schema 以 `src/db/init.rs` 为准
