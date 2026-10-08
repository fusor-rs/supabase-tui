select
  coalesce(email, '') as "Email",
  coalesce(phone, '') as "Phone",
  coalesce(raw_app_meta_data ->> 'provider', '') as "Provider",
  case
    when banned_until > now() then 'banned'
    when email_confirmed_at is null and phone_confirmed_at is null then 'unconfirmed'
    else 'active'
  end as "Status",
  to_char(created_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI') as "Created (UTC)",
  coalesce(
    to_char(last_sign_in_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI'),
    'never'
  ) as "Last sign-in (UTC)",
  id as "ID"
from auth.users
where $1 = ''
  or strpos(lower(concat_ws(' ', email, phone, id::text)), lower($1)) > 0
order by created_at desc
limit 500
