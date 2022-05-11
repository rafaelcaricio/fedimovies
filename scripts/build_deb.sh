#!/bin/sh

set -e
set -x

VERSION=$(cargo metadata --quiet --no-deps --offline | jq -r ".packages[0].version")
ARCH=$(dpkg --print-architecture)

PACKAGE_DIR="target/debian/tmp"
WEB_DIR="$1"

# Package info
rm -rf $PACKAGE_DIR
mkdir -p $PACKAGE_DIR/debian
cp contrib/debian/* $PACKAGE_DIR/debian/
sed -i "s/0.0.0/${VERSION}/" $PACKAGE_DIR/debian/changelog
echo "Architecture: $ARCH" >> $PACKAGE_DIR/debian/control

# Service
cp contrib/mitra.service $PACKAGE_DIR/debian/mitra.service

# Config file
mkdir -p $PACKAGE_DIR/etc/mitra
cp contrib/mitra_config.yaml $PACKAGE_DIR/etc/mitra/config.yaml

# Config example
mkdir -p $PACKAGE_DIR/usr/share/mitra/examples
cp contrib/mitra_config.yaml $PACKAGE_DIR/usr/share/mitra/examples/config.yaml

# Binaries
mkdir -p $PACKAGE_DIR/usr/bin
cp target/release/mitra $PACKAGE_DIR/usr/bin/mitra
cp target/release/mitractl $PACKAGE_DIR/usr/bin/mitractl

# Webapp
mkdir -p $PACKAGE_DIR/usr/share/mitra
# https://people.debian.org/~neilm/webapps-policy/ch-issues.html#s-issues-fhs
cp -r $WEB_DIR $PACKAGE_DIR/usr/share/mitra/www

# Build
cd $PACKAGE_DIR
dpkg-buildpackage --build=binary --unsigned-changes
