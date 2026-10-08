select
  namespace.nspname as "Schema",
  class.relname as "Table",
  greatest(class.reltuples, 0)::bigint as "Rows (est.)",
  pg_size_pretty(pg_total_relation_size(class.oid)) as "Size",
  case when class.relrowsecurity then 'enabled' else 'DISABLED' end as "RLS"
from pg_class as class
join pg_namespace as namespace on namespace.oid = class.relnamespace
where class.relkind in ('r', 'p')
  and not class.relispartition
  and namespace.nspname <> 'information_schema'
  and namespace.nspname not like 'pg\_%'
order by namespace.nspname <> 'public', namespace.nspname, class.relname
