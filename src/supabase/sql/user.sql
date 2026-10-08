select
  users.id as "ID",
  coalesce(users.email, '') as "Email",
  coalesce(users.phone, '') as "Phone",
  coalesce(
    (
      select string_agg(identities.provider, ', ' order by identities.provider)
      from auth.identities as identities
      where identities.user_id = users.id
    ),
    ''
  ) as "Providers",
  coalesce(
    to_char(users.email_confirmed_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI'),
    'not confirmed'
  ) as "Email confirmed (UTC)",
  case
    when users.banned_until > now()
      then to_char(users.banned_until at time zone 'UTC', 'YYYY-MM-DD HH24:MI')
    else 'not banned'
  end as "Banned until (UTC)",
  (
    select count(*) from auth.sessions as sessions where sessions.user_id = users.id
  ) as "Active sessions",
  (
    select count(*)
    from auth.mfa_factors as factors
    where factors.user_id = users.id and factors.status = 'verified'
  ) as "MFA factors",
  to_char(users.created_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI') as "Created (UTC)",
  coalesce(
    to_char(users.last_sign_in_at at time zone 'UTC', 'YYYY-MM-DD HH24:MI'),
    'never'
  ) as "Last sign-in (UTC)",
  users.raw_app_meta_data as "App metadata",
  users.raw_user_meta_data as "User metadata"
from auth.users as users
where users.id = $1::uuid
