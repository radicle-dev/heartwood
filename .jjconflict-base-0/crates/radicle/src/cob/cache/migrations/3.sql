create index if not exists 'ix_issues_repo_id' on 'issues' (repo, id);
create index if not exists 'ix_patches_repo_id' on 'patches' (repo, id);
