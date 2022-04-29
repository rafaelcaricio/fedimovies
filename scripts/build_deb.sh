#!/bin/sh

set -e
set -x

VERSION=$(cargo metadata --quiet --no-deps --offline | jq -r ".packages[0].version")
ARCH=$(dpkg --print-architecture)

PACKAGE_DIR="target/debian/tmp"
WEB_DIR="$1"

rm -rf $PACKAGE_DIR
mkdir -p $PACKAGE_DIR/DEBIAN
cp contrib/debian_control_file $PACKAGE_DIR/DEBIAN/control
echo "Version: $VERSION" >> $PACKAGE_DIR/DEBIAN/control
echo "Architecture: $ARCH" >> $PACKAGE_DIR/DEBIAN/control

mkdir -p $PACKAGE_DIR/usr/bin
cp target/release/mitra $PACKAGE_DIR/usr/bin/mitra
cp target/release/mitractl $PACKAGE_DIR/usr/bin/mitractl

mkdir -p $PACKAGE_DIR/usr/share/mitra
# https://people.debian.org/~neilm/webapps-policy/ch-issues.html#s-issues-fhs
cp -r $WEB_DIR $PACKAGE_DIR/usr/share/mitra/www

dpkg-deb --build target/debian/tmp target/debian/mitra_$VERSION_$ARCH.deb
