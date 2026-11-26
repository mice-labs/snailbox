.PHONY: check

# The repo holds only docs so far, so check covers the tracked Markdown: ADR
# naming and status, relative links, and whitespace errors against HEAD.
# Relative links are found only in the inline `[text](path)` form.
check:
	@fail=0; \
	for f in docs/adr/*.md; do \
		[ -e "$$f" ] || continue; \
		case "$$(basename "$$f")" in \
			[0-9][0-9][0-9][0-9]-*.md) ;; \
			*) echo "$$f: not named NNNN-<slug>.md"; fail=1 ;; \
		esac; \
		awk 'NR==1 && $$0!="---"{exit 1} NR>1 && $$0=="---"{exit 1} /^status:/{found=1; exit} END{exit !found}' "$$f" \
			|| { echo "$$f: no status: line in frontmatter"; fail=1; }; \
	done; \
	dups=$$(ls docs/adr 2>/dev/null | sed -n 's/^\([0-9]\{4\}\)-.*\.md$$/\1/p' | sort | uniq -d); \
	[ -z "$$dups" ] || { echo "docs/adr: duplicate ADR number(s): $$dups"; fail=1; }; \
	for f in $$(git ls-files '*.md'); do \
		for l in $$(command grep -o '\]([^)#[:space:]]*' "$$f" | sed 's/^\](//' | command grep -vE '^([a-z][a-z0-9+.-]*:|$$)'); do \
			[ -e "$$(dirname "$$f")/$$l" ] || { echo "$$f: broken link: $$l"; fail=1; }; \
		done; \
	done; \
	git diff --check HEAD || fail=1; \
	exit $$fail
