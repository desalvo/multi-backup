FROM mariadb:11.4
# Dedicated disposable CI image with a real Galera 4 provider and rsync SST.
RUN set -eux; \
    apt-get update -qq; \
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends galera-4 rsync socat; \
    lib="$(find /usr/lib -name libgalera_smm.so -print -quit)"; \
    test -n "$lib"; \
    mkdir -p /usr/lib/galera; \
    if [ "$lib" != "/usr/lib/galera/libgalera_smm.so" ]; then ln -s "$lib" /usr/lib/galera/libgalera_smm.so; fi; \
    rm -rf /var/lib/apt/lists/*
