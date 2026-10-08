with entries as (
  select
    name,
    split_part(substr(name, char_length($2) + 1), '/', 1) as entry,
    strpos(substr(name, char_length($2) + 1), '/') > 0 as is_folder,
    metadata,
    updated_at
  from storage.objects
  where bucket_id = $1 and starts_with(name, $2)
)
select
  entry || case when is_folder then '/' else '' end as "Name",
  case
    when is_folder
      then (count(*) filter (where name not like '%/.emptyFolderPlaceholder')) || ' files'
    else max(metadata ->> 'mimetype')
  end as "Type",
  pg_size_pretty(sum((metadata ->> 'size')::bigint)) as "Size",
  to_char(max(updated_at) at time zone 'UTC', 'YYYY-MM-DD HH24:MI') as "Updated (UTC)"
from entries
where entry <> '.emptyFolderPlaceholder'
group by entry, is_folder
order by is_folder desc, entry
limit 1000
