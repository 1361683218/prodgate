// After supabase/supabase@49bda396 — the two missing statements added
// prodgate: rls=enabled
alter table public.user_roles enable row level security;
alter table public.role_permissions enable row level security;
