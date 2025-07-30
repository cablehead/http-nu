# Claude Development Notes

## Git Commit Style Preferences

- Use conventional commit format: `type: subject line`
- Keep subject line concise and descriptive
- No marketing language or promotional text in commit messages
- No "Generated with Claude" or similar attribution in commit messages
- Follow existing project patterns from git log

Example good commit messages from this project:

- `test: allow dead code in test utility methods`
- `fix: improve error handling`
- `feat: add a --fallback option to .static to support SPAs`
- `refactor: remove axum dependency, consolidate unix socket, tcp and tls handling`

## Check Script

Run `./scripts/check.sh` to verify code quality before committing. This runs:

- deno fmt README.md --check
- cargo fmt --check --all
- cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
- cargo test

## Release Creation Process

To create a new release:

1. **Identify last stable release**: Find the last stable release tag (exclude
   dev tags)
   ```bash
   git tag --sort=-version:refname | grep -v dev | head -1
   ```

2. **Get commit history**: Collect commit subject lines since the last stable
   release
   ```bash
   git log --oneline --pretty=format:"* %s (%ad)" --date=short v0.4.3..HEAD
   ```

3. **Create change notes file**: Create `changes/v{version}.md` with:
   - Title: `# v{version}`
   - Raw commits section with the commit list from step 2

4. **Review and add highlights**:
   - Review the raw commits for interesting user-facing changes
   - Use `git diff` to understand significant changes
   - Add a "## Highlights" section above "## Raw commits" for notable
     features/fixes

5. **Tag the release**: After committing the change notes, tag the release
   ```bash
   git tag v{version}
   ```

The GitHub workflow will automatically build and create the release when the tag
is pushed.
