# Local development for the Brave Bot documentation site.
#
# The site is plain Docusaurus, so targets here are thin wrappers over npm scripts.

.PHONY: help
help:
	@echo "Brave Bot documentation"
	@echo
	@echo "Development:"
	@echo "  make init                 Install dependencies"
	@echo "  make install              Install dependencies only"
	@echo "  make start                Serve locally with live reload"
	@echo "  make build                Build the static site into build/"
	@echo "  make serve                Serve the built site from build/"
	@echo "  make check                What CI checks: a clean build with no broken links"
	@echo "  make clear                Drop the Docusaurus cache"
	@echo "  make clean                Drop build output, cache, and node_modules"

.PHONY: init
init: install

.PHONY: install
install:
	npm install

.PHONY: start
start:
	npm start

.PHONY: build
build:
	npm run build

.PHONY: serve
serve:
	npm run serve

# docusaurus.config.js throws on a broken link or anchor, so a clean build is the whole of
# the site's correctness check. Named separately from `build` because that is what it is
# used for, and so CI and a person reach for the same word.
.PHONY: check
check: build

.PHONY: clear
clear:
	npm run clear

.PHONY: clean
clean:
	rm -rf build .docusaurus .cache-loader node_modules
