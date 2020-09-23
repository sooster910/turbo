FROM troian/golang-cross-builder:v1.15.2

LABEL maintainer="Artur Troian <troian dot ap at gmail dot com>"

COPY entrypoint.sh /

ARG GORELEASER_VERSION=0.143.0
ARG GORELEASER_SHA=cc435eb337889d41414de80fd8474806187a3e908754cbf4599aa0a7604a3134

RUN \
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg | APT_KEY_DONT_WARN_ON_DANGEROUS_USAGE=1 apt-key add - \
 && add-apt-repository "deb [arch=amd64] https://download.docker.com/linux/debian $(lsb_release -cs) stable" \
 && apt-get update \
 && apt-get --no-install-recommends -y install docker-ce docker-ce-cli \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/* \
 && GORELEASER_DOWNLOAD_FILE=goreleaser_Linux_x86_64.tar.gz \
 && GORELEASER_DOWNLOAD_URL=https://github.com/goreleaser/goreleaser/releases/download/v${GORELEASER_VERSION}/${GORELEASER_DOWNLOAD_FILE} \
 && wget ${GORELEASER_DOWNLOAD_URL} \
 && echo "$GORELEASER_SHA $GORELEASER_DOWNLOAD_FILE" | sha256sum -c - || exit 1 \
 && tar -xzf $GORELEASER_DOWNLOAD_FILE -C /usr/bin/ goreleaser \
 && rm $GORELEASER_DOWNLOAD_FILE \
 && go get -u github.com/git-chglog/git-chglog/cmd/git-chglog \
 && chmod +x /entrypoint.sh

ENTRYPOINT ["bash", "/entrypoint.sh"]
