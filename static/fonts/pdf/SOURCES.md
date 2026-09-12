# Fonts embedded in lesson PDFs

Static WOFF instances of the desk's three families, built by
`scripts/build-pdf-fonts.py` from the variable fonts in the Google Fonts
repository (`ofl/inter`, `ofl/jetbrainsmono`, `ofl/fraunces`), pinned to one
weight each and subset to Latin plus the arrow, operator, technical, shape and
dingbat blocks that lessons use. The screen fonts in the parent folder are
split by script and lack those symbols, which is why these exist.

All three families are licensed under the SIL Open Font License 1.1; the
licence texts are in the parent folder.

| File | Family | Weight |
| --- | --- | --- |
| inter-regular.woff, inter-bold.woff, inter-italic.woff, inter-bold-italic.woff | Inter (optical size 14) | 400, 700 |
| jetbrains-mono-regular.woff, jetbrains-mono-bold.woff | JetBrains Mono | 400, 700 |
| fraunces-medium.woff, fraunces-semibold.woff | Fraunces (optical size 24, SOFT 0, WONK 0) | 500, 600 |
