#!/bin/sh

set -e
set -x

VERSION=$(cargo metadata --quiet --no-deps --offline | jq -r ".packages[0].version")
ARCH=$(dpkg --print-architecture)

PACKAGE_DIR="target/debian/tmp"
WEB_DIR="$1"

# Package info
rm -rf $PACKAGE_DIR
mkdir -p $PACKAGE_DIR/DEBIAN
cp contrib/debian/* $PACKAGE_DIR/DEBIAN/
echo "Version: $VERSION" >> $PACKAGE_DIR/DEBIAN/control
echo "Architecture: $ARCH" >> $PACKAGE_DIR/DEBIAN/control

# Binaries
mkdir -p $PACKAGE_DIR/usr/bin
cp target/release/mitra $PACKAGE_DIR/usr/bin/mitra
cp target/release/mitractl $PACKAGE_DIR/usr/bin/mitractl

# Config directory
mkdir -p $PACKAGE_DIR/etc/mitra

# Config example
mkdir -p $PACKAGE_DIR/usr/share/mitra/examples
cp config.yaml.example $PACKAGE_DIR/usr/share/mitra/examples/config.yaml

# Service
mkdir -p $PACKAGE_DIR/lib/systemd/system
cp contrib/mitra.service $PACKAGE_DIR/lib/systemd/system/mitra.service

# Webapp
mkdir -p $PACKAGE_DIR/usr/share/mitra
# https://people.debian.org/~neilm/webapps-policy/ch-issues.html#s-issues-fhs
cp -r $WEB_DIR $PACKAGE_DIR/usr/share/mitra/www

dpkg-deb --build target/debian/tmp target/debian/mitra_${VERSION}_${ARCH}.deb
