#!/usr/bin/env python3
"""Validate the static project website without a JavaScript toolchain or network.

Versioned download links match website/downloads.toml, not the source version:
platform candidates can advance independently and unpublished drafts must never
create broken public links. GitHub assets are verified during publication.
"""
from html.parser import HTMLParser
from pathlib import Path
import re
import tomllib
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / 'website'
REPO = 'https://github.com/borjaburgos/cartridge-studio'


class Page(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ids = set()
        self.links = []
        self.downloads = {}
        self.errors = []
        self.lang = None
        self.has_title = False
        self.has_viewport = False

    def handle_starttag(self, tag, attributes):
        attrs = dict(attributes)
        if tag == 'html':
            self.lang = attrs.get('lang')
        if tag == 'title':
            self.has_title = True
        if tag == 'meta' and attrs.get('name') == 'viewport':
            self.has_viewport = True
        if 'id' in attrs:
            if attrs['id'] in self.ids:
                self.errors.append(f'Duplicate ID: {attrs["id"]}')
            self.ids.add(attrs['id'])
        if tag == 'img' and 'alt' not in attrs:
            self.errors.append(f'Missing image alt text: {attrs.get("src")}')
        for attribute in ('href', 'src'):
            if attribute in attrs:
                self.links.append(attrs[attribute])
        if 'data-download' in attrs:
            self.downloads[attrs['data-download']] = attrs.get('href')


def main():
    releases = tomllib.loads((SITE / 'downloads.toml').read_text())
    linux = releases['linux']
    version = linux['version']
    architecture = linux['architecture']
    if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:-(?:alpha|beta|rc)\.[0-9]+)?', version):
        raise SystemExit(f'Invalid pinned Linux release version: {version!r}')
    if architecture not in ('x86_64', 'aarch64'):
        raise SystemExit(f'Unexpected Linux architecture: {architecture!r}')
    page = Page()
    page.feed((SITE / 'index.html').read_text())
    errors = page.errors
    if not (page.lang == 'en' and page.has_title and page.has_viewport):
        errors.append('Missing language, title or responsive viewport metadata')
    expected = {
        'portable': f'{REPO}/releases/download/v{version}/cartridge-studio-{version}-linux-{architecture}.tar.gz',
        'release': f'{REPO}/releases/tag/v{version}',
        'checksums': f'{REPO}/releases/download/v{version}/SHA256SUMS',
    }
    if page.downloads != expected:
        errors.append(f'Download links must match the {version} release: {expected}')
    for link in page.links:
        parsed = urlsplit(link)
        if parsed.scheme not in ('', 'https'):
            errors.append(f'Unexpected URL scheme: {link}')
        if not parsed.scheme:
            if parsed.path:
                path = (SITE / unquote(parsed.path)).resolve()
                if not path.is_relative_to(SITE.resolve()) or not path.is_file():
                    errors.append(f'Missing or out-of-tree asset: {link}')
            elif parsed.fragment and parsed.fragment not in page.ids:
                errors.append(f'Missing page anchor: {link}')
        prefix = REPO + '/blob/main/'
        if link.startswith(prefix):
            relative = unquote(parsed.path.split('/blob/main/', 1)[1])
            path = ROOT / relative
            if not path.is_file():
                errors.append(f'Missing repository document: {relative}')
            elif parsed.fragment:
                headings = re.findall(r'^#{1,6}\s+(.+)$', path.read_text(), re.M)
                anchors = [re.sub(r'[^\w\- ]', '', h.lower()).replace(' ', '-') for h in headings]
                explicit = re.findall(r'<a\s+(?:id|name)="([^"]+)"', path.read_text())
                if parsed.fragment not in anchors + explicit:
                    errors.append(f'Missing documentation anchor: {link}')
    for path in SITE.rglob('*'):
        if path.is_file() and path.suffix.lower() in ('.gb', '.gbc', '.gba', '.nes', '.sav', '.bin', '.env'):
            errors.append(f'Private cartridge data does not belong on the website: {path.name}')
    if errors:
        raise SystemExit('\n'.join(errors))
    print(f'Website checked: {len(page.links)} links/assets, accessible image labels, Linux release v{version}.')


if __name__ == '__main__':
    main()
