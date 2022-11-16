# CONTRIBUTING

Contributions are very welcome. When contributing code, please follow these
simple guidelines.

* Make sure you run `rustfmt` on your code. Also ensure all trailing whitespace is trimmed.
* Run the tests with `cargo test --all`.
* Before adding any code dependencies, check with the maintainers if this is okay.
* Write properly formatted comments: they should be English sentences, eg:

        // Return the current UNIX time.

* Read the DCO and make sure all commits are signed off, using `git commit -s`.
* Follow the guidelines when proposing code changes (see below).
* Write properly formatted git commits (see below).

Proposing changes
-----------------
When proposing changes via a pull-request or patch:

* Isolate changes in separate commits to make the review process easier.
* Don't make unrelated changes, unless it happens to be an obvious improvement to
  code you are touching anyway ("boyscout rule").
* Rebase on `master` when needed.
* Keep your changesets small, specific and uncontroversial, so that they can be
  merged more quickly.
* If the change is substantial or requires re-architecting certain parts of the
  codebase, write a proposal in English first, and get consensus on that before
  proposing the code changes.

Writing Git commit messages
---------------------------
A properly formed git commit subject line should always be able to complete the
following sentence:

     If applied, this commit will _____

In addition, it should be capitalized and *must not* include a period.

For example, the following message is well formed:

     Add support for .gif files

While these ones are **not**: `Adding support for .gif files`,
`Added support for .gif files`.

When it comes to formatting, here's a model git commit message[1]:

     Capitalized, short (50 chars or less) summary

     More detailed explanatory text, if necessary.  Wrap it to about 72
     characters or so.  In some contexts, the first line is treated as the
     subject of an email and the rest of the text as the body.  The blank
     line separating the summary from the body is critical (unless you omit
     the body entirely); tools like rebase can get confused if you run the
     two together.

     Write your commit message in the imperative: "Fix bug" and not "Fixed bug"
     or "Fixes bug."  This convention matches up with commit messages generated
     by commands like git merge and git revert.

     Further paragraphs come after blank lines.

     - Bullet points are okay, too.

     - Typically a hyphen or asterisk is used for the bullet, followed by a
       single space, with blank lines in between, but conventions vary here.

     - Use a hanging indent.

---

[1]: http://tbaggery.com/2008/04/19/a-note-about-git-commit-messages.html
