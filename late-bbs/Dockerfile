# syntax=docker/dockerfile:1.7
#
# Experimental Synchronet + dosemu2 image for the LORD BBS V1 stack.
# This image must not include LORD files. Install the registered BBS door into
# the mounted data directory at /bbs/doors/lord after purchase.

ARG UBUNTU_VERSION=24.04
ARG SBBS_TAG=sbbs321e

FROM ubuntu:${UBUNTU_VERSION} AS synchronet-build

ARG SBBS_TAG

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    make \
    gcc \
    g++ \
    libc6-dev \
    libarchive-dev \
    libncurses-dev \
    libncursesw5-dev \
    libnspr4-dev \
    patch \
    perl \
    pkg-config \
    python3 \
    sudo \
    unzip \
    wget \
    zip \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /bin/bash sbbs \
    && mkdir -p /opt/sbbs \
    && chown -R sbbs:sbbs /opt/sbbs

USER sbbs
WORKDIR /opt/sbbs

RUN wget -O install-sbbs.mk https://gitlab.synchro.net/main/sbbs/-/raw/master/install/install-sbbs.mk \
    && make -f install-sbbs.mk SYMLINK=1 NOCAP=1 TAG="${SBBS_TAG}"

FROM ubuntu:${UBUNTU_VERSION} AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    gosu \
    libarchive13 \
    libncursesw6 \
    libnspr4 \
    libssl3t64 \
    procps \
    unzip \
    wget \
    && rm -rf /var/lib/apt/lists/*

# dosemu2 publishes current Ubuntu packages through Launchpad. Keep this in the
# runtime layer so the BBS image can execute DOS doors without copying DOS game
# binaries into the repo.
RUN apt-get update && apt-get install -y --no-install-recommends software-properties-common \
    && add-apt-repository -y ppa:dosemu2/ppa \
    && apt-get update \
    && apt-get install -y --no-install-recommends dosemu2 fdpp \
    && apt-get purge -y --auto-remove software-properties-common \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /bin/bash sbbs \
    && mkdir -p /opt/sbbs /bbs \
    && chown -R sbbs:sbbs /opt/sbbs /bbs

COPY --from=synchronet-build --chown=sbbs:sbbs /opt/sbbs/ /opt/sbbs/
COPY entrypoint.sh /usr/local/bin/late-bbs-entrypoint
COPY lord-runner.sh /usr/local/bin/lord-runner
RUN chmod +x /usr/local/bin/late-bbs-entrypoint /usr/local/bin/lord-runner

ENV SBBS_HOME=/bbs/sbbs
ENV LORD_HOME=/bbs/doors/lord
ENV SBBSCTRL=/bbs/sbbs/ctrl
ENV SBBSEXEC=/bbs/sbbs/exec

VOLUME ["/bbs"]
EXPOSE 23 22

ENTRYPOINT ["/usr/local/bin/late-bbs-entrypoint"]
