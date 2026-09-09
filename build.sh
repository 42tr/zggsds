#!/bin/bash
set -e

# 镜像名称和标签
IMAGE_NAME="zggsds"
IMAGE_TAG="${1:-latest}"

echo "==> 开始本地编译 Rust 项目..."
cargo build --release

echo "==> 复制二进制文件..."
# 复制到临时目录（绕过 .dockerignore 对 target/ 的忽略）
cp target/release/zggsds ./zggsds-bin

echo "==> 开始构建 Docker 镜像..."

# 使用临时 Dockerfile 构建运行时镜像
docker build -t "${IMAGE_NAME}:${IMAGE_TAG}" -f - . <<'EOF'
FROM debian:bookworm-slim

WORKDIR /app

# 安装运行时依赖
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# 复制本地编译好的二进制文件
COPY zggsds-bin ./zggsds

# 创建数据目录
RUN mkdir -p /app/data && \
    chmod +x /app/zggsds && \
    chown -R nobody:nogroup /app

# 切换到非 root 用户
USER nobody:nogroup

# 暴露端口
EXPOSE 3000

# 设置环境变量
ENV RUST_LOG=info

# 运行应用
ENTRYPOINT ["./zggsds"]
EOF

# 清理临时文件
rm -f ./zggsds-bin

echo "==> 构建完成: ${IMAGE_NAME}:${IMAGE_TAG}"
echo ""
echo "运行镜像: docker run -p 3000:3000 -v \$(pwd)/data:/app/data ${IMAGE_NAME}:${IMAGE_TAG}"
