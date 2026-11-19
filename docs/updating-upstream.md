# Updating the Pinned `upstream/Rin-UI` Submodule

`upstream/Rin-UI` is pinned to a specific commit SHA for reproducible builds and must **never** be modified directly inside the submodule.

## Procedure

1. **Fetch and inspect the desired upstream revision**:
   ```bash
   git -C upstream/Rin-UI fetch origin
   git -C upstream/Rin-UI log --oneline HEAD..origin/master
   git -C upstream/Rin-UI checkout <target-commit-or-tag>
   ```

2. **Run the upstream audit script**:
   ```bash
   python3 scripts/audit_upstream.py
   ```
   - The audit script checks that `upstream/Rin-UI` has zero local modifications.
   - If upstream fixed previously broken `qmldir` entries (`DateField`, `TeachingTip`, `ScrollViewer`, `HyperlinkButton`, `ErrorPage`), it prints a notice so you can update `EXPECTED_BROKEN_QMLDIR_ENTRIES` in `scripts/audit_upstream.py`.
   - If upstream introduced **new** broken `qmldir` entries or changed host boundary calls (`ThemeManager`, `WinEventManager`), the audit flags them before compilation.

3. **Diff `upstream/Rin-UI/RinUI/core/` for host contract changes**:
   ```bash
   git -C upstream/Rin-UI diff <old-sha>..<new-sha> -- RinUI/core/ RinUI/qmldir RinUI/*/qmldir
   ```
   Check for any new QML slots, signals, window properties, configuration keys, or required Qt modules, and update:
   - `crates/rinui-host/src/` & `native/`
   - `crates/rinui-host/build.rs` (sanitization rules if needed)
   - `docs/compatibility.md`

4. **Rebuild and validate**:
   ```bash
   cargo test --workspace
   cargo build --release -p rinui-gallery
   ```
   Also run the isolated binary smoke test and native UI check on each target OS.

5. **Commit the updated submodule pointer in `rinui-rs`**:
   ```bash
   git add upstream/Rin-UI docs/compatibility.md
   git commit -m "chore: bump upstream/Rin-UI to <short-sha>"
   ```
   Never commit changes inside `upstream/Rin-UI` itself.
