# Third-party notices

vma is MIT licensed. It is compiled from the Rust crates below, whose
licences ask that their notices travel with the software they are built into.
That is what this file is for, and `vma notices` prints it, as nothing ships
beside the executable. Every one of them offers MIT among its choices, and is
used under MIT.

A crate added to `Cargo.toml` is added here in the same commit, with its
licence as `cargo tree -e normal --target x86_64-pc-windows-gnu --format
'{p} {l}'` reports it.

---

## MIT or MIT/Apache-2.0

| Crate | Licence |
| --- | --- |
| `windows` and its `windows-*` companions (`windows-core`, `windows-collections`, `windows-future`, `windows-implement`, `windows-interface`, `windows-link`, `windows-numerics`, `windows-result`, `windows-strings`, `windows-threading`) | MIT OR Apache-2.0 |
| `proc-macro2`, `quote`, `syn` | MIT OR Apache-2.0 |
| `unicode-ident` | (MIT OR Apache-2.0) AND Unicode-3.0 |

> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

Copyright for each crate remains with its own authors; the individual notices
are in the crates' own repositories, reachable from <https://crates.io>.
`unicode-ident` carries Unicode's data under the Unicode License v3, whose
text is at <https://www.unicode.org/license.txt>.
