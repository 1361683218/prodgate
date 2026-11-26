// Before supabase/supabase@49bda396 — from the RBAC guide's schema block
// prodgate: rls=off
create table public.user_roles (
  user_id uuid references auth.users,
  role text
);
