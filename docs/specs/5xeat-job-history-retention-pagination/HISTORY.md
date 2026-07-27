# History

- Introduced to govern production SQLite growth observed on server 101 without changing deployment configuration or automatically running `VACUUM`.
- The 101 runtime showed that hourly 10,000-row resource GC could not catch up with raw sampling. Resource retention was tightened to 24 hours and runtime drain was bounded to ten batches per minute; database compaction remains explicitly outside the runtime path.
