FROM ubuntu:23.04

RUN apt-get update && apt-get install -y \
    curl \
    wget \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /var/lib/data

COPY build/fedimovies /usr/local/bin
COPY build/fedimoviesctl /usr/local/bin
COPY secret/fedimovies.conf /etc/fedimovies.conf
COPY files /www/frontend/

CMD ["/usr/local/bin/fedimovies"]
