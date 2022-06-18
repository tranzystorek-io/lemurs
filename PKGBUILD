# Maintainer: Gijs Burghoorn <me@gburghoorn.com>
pkgname='lemurs-git'
pkgver='0.2.0'
pkgrel=1
pkgdesc="TUI Display/Login Manager written in Rust"
arch=('x86_64')
url="https://github.com/coastalwhite/lemurs"
license=('MIT', 'APACHE')
makedepends=('git' 'cargo')
install='AUR.install'
changelog='CHANGELOG.md'
source=("lemurs-$pkgver::git+https://github.com/coastalwhite/lemurs.git")
md5sums=('SKIP')

prepare() {
	cd "lemurs-$pkgver"
	cargo fetch --locked --target "$CARCH-unknown-linux-gnu"
}

build() {
	cd "lemurs-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --frozen --release --all-features
}

package() {
    install -Dm755 -t "$pkgdir/etc/lemurs" "$srcdir/lemurs-$pkgver/extra/xsetup.sh"
    install -Dm755 -t "$pkgdir/usr/lib/systemd/system" "$srcdir/lemurs-$pkgver/extra/lemurs.service"

    install -Dm755 -t "$pkgdir/usr/bin/" "$srcdir/lemurs-$pkgver/target/release/lemurs"
    install -Dm644 -t "$pkgdir/usr/share/licenses/$pkgname/" "$srcdir/lemurs-$pkgver/LICENSE-MIT"
    install -Dm644 -t "$pkgdir/usr/share/licenses/$pkgname/" "$srcdir/lemurs-$pkgver/LICENSE-APACHE"
    install -Dm644 -t "$pkgdir/usr/share/doc/$pkgname/" "$srcdir/lemurs-$pkgver/README.md"
}
