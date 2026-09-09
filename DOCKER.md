# Docker 部署指南

## 镜像构建

### GitHub Actions 发布

仅推送 tag 时触发工作流；普通分支推送和 PR 不触发。
Rust 检查任务与两个原生编译任务并行运行：`ubuntu-24.04` 编译 amd64，
`ubuntu-24.04-arm` 编译 arm64，各自缓存 Cargo 依赖和编译产物。
全部通过后，下载二进制产物，使用 `Dockerfile.release` 打包
`linux/amd64`、`linux/arm64` 双架构镜像，并推送到：

- `ghcr.io/42tr/zggsds`
- `crpi-gz6f3ok0ezphywc8.cn-shanghai.personal.cr.aliyuncs.com/42tr/zggsds`

镜像标签为去掉开头 `v` 的 Git tag 和 `latest`，例如 `v0.0.2` 发布为
`0.0.2` 和 `latest`。Git tag 应使用合法的 Docker 标签格式。

在本仓库的 Actions Secrets 中配置 `ALIYUN_REGISTRY_USERNAME` 和
`ALIYUN_REGISTRY_PASSWORD`，并确保该账号有目标 ACR 仓库的推送权限。
GHCR 使用工作流自带的 `GITHUB_TOKEN`。

发布镜像使用 Ubuntu 24.04，与原生编译环境的 glibc 版本匹配。
打包阶段只复制二进制、创建数据目录，不执行容器内命令，无需 QEMU。
本地 `docker build .` 仍使用下面的多阶段 Dockerfile 从源码编译。

### 使用 Docker 构建

```bash
docker build -t zggsds:latest .
```

### 使用 Docker Compose 构建

```bash
docker-compose build
```

## 运行容器

### 使用 Docker 运行

先在当前 shell 中设置 `JWT_SECRET`（建议至少 32 字节随机值）和
`ADMIN_PASSWORD`（首次初始化管理员时必填，至少 12 位），并 export。
`make run` 和 Docker Compose 也会读取这两个环境变量。

```bash
# 创建数据目录
mkdir -p ./data

# 运行容器
docker run -d \
  --name zggsds \
  -p 3000:3000 \
  -v $(pwd)/data:/app/data \
  -e RUST_LOG=info \
  -e JWT_SECRET \
  -e ADMIN_PASSWORD \
  --restart unless-stopped \
  zggsds:latest
```

### 使用 Docker Compose 运行

```bash
docker-compose up -d
```

## 查看日志

```bash
# 查看容器日志
docker logs -f zggsds

# 使用 Docker Compose
docker-compose logs -f
```

## 停止服务

```bash
# 停止容器
docker stop zggsds

# 使用 Docker Compose
docker-compose down
```

## 镜像优化说明

本项目使用多阶段构建（Multi-stage build）策略：

1. **构建阶段** (rust:1.88.0-slim-bookworm，与 CI 的 Rust 版本一致)
   - 完整的 Rust 工具链
   - 构建依赖和二进制文件
   - 利用 Docker 缓存层加速构建

2. **运行阶段** (debian:bookworm-slim)
   - 最小的 Debian 基础镜像
   - 仅包含运行时所需的二进制文件
   - 使用非 root 用户运行

### 镜像大小对比

- 单阶段构建（完整 Rust 环境）：~1.5GB
- 多阶段构建：~50-80MB

## 数据持久化

数据库文件存储在容器内的 `/app/data` 目录，建议通过 Volume 挂载到宿主机：

```yaml
volumes:
  - ./data:/app/data
```

## 环境变量

| 变量名 | 说明 | 默认值 |
|--------|------|--------|
| RUST_LOG | 日志级别 | info |
| RUST_BACKTRACE | 错误回溯 | 0 |
| JWT_SECRET | JWT 签名密钥（必须设置，建议使用至少32字节随机值） | 无默认值 |
| ADMIN_PASSWORD | 首次创建 admin 账号时使用的密码（至少12位） | 无默认值 |

## 健康检查

镜像目前未配置 Docker HEALTHCHECK。可在宿主机检查 HTTP 服务是否响应：

```bash
curl --fail http://localhost:3000/ > /dev/null
```

## 生产环境建议

1. **反向代理**：使用 Nginx 或 Traefik 作为反向代理
2. **HTTPS**：配置 SSL/TLS 证书
3. **资源限制**：设置 CPU 和内存限制
4. **日志管理**：配置日志轮转和集中收集
5. **备份**：定期备份 `/app/data` 目录

### Nginx 配置示例

```nginx
server {
    listen 80;
    server_name your-domain.com;
    
    location / {
        proxy_pass http://localhost:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```
