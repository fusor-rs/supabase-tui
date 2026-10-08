select
  bucket_id as "Bucket",
  name as "Path",
  coalesce(metadata ->> 'mimetype', '') as "Type",
  pg_size_pretty((metadata ->> 'size')::bigint) as "Size",
  to_char(updated_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI') as "Updated (UTC)"
from storage.objects
where ($1 = '' or bucket_id = $1)
  and strpos(lower(name), lower($2)) > 0
  and name not like '%.emptyFolderPlaceholder'
order by updated_at desc
limit 500
