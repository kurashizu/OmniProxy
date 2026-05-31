FROM busybox:musl

COPY target/x86_64-unknown-linux-musl/release/server /usr/local/bin/server

EXPOSE 9880

ENTRYPOINT ["server"]
CMD ["--addr", "0.0.0.0", "--port", "9880", "--token", ""]
