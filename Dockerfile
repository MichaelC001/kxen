# kxen 无头 server 镜像:默认 Web 模式,浏览器客户端。
# 二进制来自 GitHub Release 资产(与桌面版同一批编译产物),构建上下文按 TARGETARCH 预置:
#   context/
#     kxen-amd64   (来自 kxen-linux-x86_64.tar.gz)
#     kxen-arm64   (来自 kxen-linux-aarch64.tar.gz)
FROM debian:bookworm-slim

# ca-certificates: rustls-native-certs 读系统根证书;git: worktree/仓库工具;bash: exec 工具探测 zsh -> bash -> sh
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git bash \
    && rm -rf /var/lib/apt/lists/*

ARG TARGETARCH
COPY kxen-${TARGETARCH} /usr/local/bin/kxen

RUN useradd --create-home kxen && mkdir -p /data && chown kxen:kxen /data
USER kxen
ENV KXEN_DATA_DIR=/data
VOLUME ["/data"]

EXPOSE 7824
# 容器内必须绑定 0.0.0.0 才能经端口映射到达;token 打印在容器日志,远程访问仍需在前面终结 TLS
ENTRYPOINT ["kxen"]
CMD ["--bind", "0.0.0.0", "--port", "7824"]
