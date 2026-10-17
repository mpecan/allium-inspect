# Changelog

## [0.1.4](https://github.com/mpecan/allium-inspect/compare/allium-inspect-v0.1.3...allium-inspect-v0.1.4) (2026-10-17)


### Bug Fixes

* **journey:** answer a module-qualified call in the module it names ([#32](https://github.com/mpecan/allium-inspect/issues/32)) ([c45775e](https://github.com/mpecan/allium-inspect/commit/c45775eefab6d3bd95c71118cf35dfbe2a2f1cb5))
* **journey:** bind every `creating` line under an act to its own name ([#31](https://github.com/mpecan/allium-inspect/issues/31)) ([c45775e](https://github.com/mpecan/allium-inspect/commit/c45775eefab6d3bd95c71118cf35dfbe2a2f1cb5))
* **sim:** read what a rule removes the same way whichever ensures line comes first ([#33](https://github.com/mpecan/allium-inspect/issues/33)) ([c45775e](https://github.com/mpecan/allium-inspect/commit/c45775eefab6d3bd95c71118cf35dfbe2a2f1cb5))

## [0.1.3](https://github.com/mpecan/allium-inspect/compare/allium-inspect-v0.1.2...allium-inspect-v0.1.3) (2026-10-16)


### Features

* **journey:** answer a black box about an instance the act creates with `where` under `creating` ([#18](https://github.com/mpecan/allium-inspect/issues/18)) ([eb3b5c4](https://github.com/mpecan/allium-inspect/commit/eb3b5c47076e4347b7580d1170af5f50e5b52a8b))
* **journey:** answer a call in `given` or `world` ([#19](https://github.com/mpecan/allium-inspect/issues/19)) ([eb3b5c4](https://github.com/mpecan/allium-inspect/commit/eb3b5c47076e4347b7580d1170af5f50e5b52a8b))
* **journey:** check a bare word against the states its field declares ([7c262c4](https://github.com/mpecan/allium-inspect/commit/7c262c4af9a53f41fa2b34238311e4fe6915afbc))
* **model:** say before anything runs when a listener can never bind an emission ([7c262c4](https://github.com/mpecan/allium-inspect/commit/7c262c4af9a53f41fa2b34238311e4fe6915afbc))
* **sim:** decide a precondition written inside a rule-level loop ([7c262c4](https://github.com/mpecan/allium-inspect/commit/7c262c4af9a53f41fa2b34238311e4fe6915afbc))


### Bug Fixes

* let the receipt gates survive a squash merge ([7c262c4](https://github.com/mpecan/allium-inspect/commit/7c262c4af9a53f41fa2b34238311e4fe6915afbc))
* **sim:** a temporal trigger on a null deadline is not due ([#20](https://github.com/mpecan/allium-inspect/issues/20)) ([eb3b5c4](https://github.com/mpecan/allium-inspect/commit/eb3b5c47076e4347b7580d1170af5f50e5b52a8b))
* **sim:** apply an emission qualified with another module ([#17](https://github.com/mpecan/allium-inspect/issues/17)) ([eb3b5c4](https://github.com/mpecan/allium-inspect/commit/eb3b5c47076e4347b7580d1170af5f50e5b52a8b))
* **sim:** compute a filtered member's derived fields in its own scope ([#23](https://github.com/mpecan/allium-inspect/issues/23)) ([eb3b5c4](https://github.com/mpecan/allium-inspect/commit/eb3b5c47076e4347b7580d1170af5f50e5b52a8b))
* **sim:** decide an `if` that returns a state word, and keep a comparison with an undecided side undecided ([#21](https://github.com/mpecan/allium-inspect/issues/21)) ([eb3b5c4](https://github.com/mpecan/allium-inspect/commit/eb3b5c47076e4347b7580d1170af5f50e5b52a8b))
* **sim:** evaluate `??` and string `+`, and keep a written value's reason ([#27](https://github.com/mpecan/allium-inspect/issues/27)) ([eb3b5c4](https://github.com/mpecan/allium-inspect/commit/eb3b5c47076e4347b7580d1170af5f50e5b52a8b))
* **sim:** iterate a set literal of state words ([#26](https://github.com/mpecan/allium-inspect/issues/26)) ([eb3b5c4](https://github.com/mpecan/allium-inspect/commit/eb3b5c47076e4347b7580d1170af5f50e5b52a8b))
* **sim:** let a `_` listener parameter take a named argument ([#16](https://github.com/mpecan/allium-inspect/issues/16)) ([eb3b5c4](https://github.com/mpecan/allium-inspect/commit/eb3b5c47076e4347b7580d1170af5f50e5b52a8b))
* **sim:** read a collection pluralised with `es` as its entity ([eb3b5c4](https://github.com/mpecan/allium-inspect/commit/eb3b5c47076e4347b7580d1170af5f50e5b52a8b))
* **sim:** seed set-literal config defaults ([#25](https://github.com/mpecan/allium-inspect/issues/25)) ([eb3b5c4](https://github.com/mpecan/allium-inspect/commit/eb3b5c47076e4347b7580d1170af5f50e5b52a8b))
* **sim:** sibling rules read the world as the trigger found it ([#24](https://github.com/mpecan/allium-inspect/issues/24)) ([eb3b5c4](https://github.com/mpecan/allium-inspect/commit/eb3b5c47076e4347b7580d1170af5f50e5b52a8b))
* **sim:** stop evaluating an instance removed while the clock moves ([#22](https://github.com/mpecan/allium-inspect/issues/22)) ([eb3b5c4](https://github.com/mpecan/allium-inspect/commit/eb3b5c47076e4347b7580d1170af5f50e5b52a8b))
* **ui:** key a journey line's outcomes by their reason too ([eb3b5c4](https://github.com/mpecan/allium-inspect/commit/eb3b5c47076e4347b7580d1170af5f50e5b52a8b))

## [0.1.2](https://github.com/mpecan/allium-inspect/compare/allium-inspect-v0.1.1...allium-inspect-v0.1.2) (2026-10-16)


### Bug Fixes

* **journey:** close the eight walk gaps found distilling Forgejo ([#6](https://github.com/mpecan/allium-inspect/issues/6)–[#13](https://github.com/mpecan/allium-inspect/issues/13)) ([#14](https://github.com/mpecan/allium-inspect/issues/14)) ([d76b7af](https://github.com/mpecan/allium-inspect/commit/d76b7af8654b310ccfddc60f2955b9f670bcce4c))

## [0.1.1](https://github.com/mpecan/allium-inspect/compare/allium-inspect-v0.1.0...allium-inspect-v0.1.1) (2026-09-25)


### Bug Fixes

* **journey:** walk what friend-mesh's specs write, and stop a state rule running on another's condition ([13ef520](https://github.com/mpecan/allium-inspect/commit/13ef520a3a9048583902962f3c6860c57dab932f))
* **journey:** walk what friend-mesh's specs write, and stop a state rule running on another's condition ([a7345cf](https://github.com/mpecan/allium-inspect/commit/a7345cfed2a8a96234e880378555f282b515c6e7))
* **model:** the version tests use a stub, not echo and true ([f1c6d54](https://github.com/mpecan/allium-inspect/commit/f1c6d5474538d2e9adccde67274690eddf4c8f20))
* **model:** the version tests use a stub, not echo and true ([2da7fe3](https://github.com/mpecan/allium-inspect/commit/2da7fe3d3ef05aa737f964eb1bacc635d7a5bb80))
* **sim:** a state rule finds its instance whatever name the firing carries it under ([690397c](https://github.com/mpecan/allium-inspect/commit/690397c975f58fbfe57dae83153bba3b7e8d5ee9))
* **ui:** a word the spec declares as a state is sent as one, and a removal is shown ([75cf3bd](https://github.com/mpecan/allium-inspect/commit/75cf3bd12c58339f94fe3ec6a3f860b04591bb02))
