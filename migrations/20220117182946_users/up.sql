begin;
    create table poop.users (
        id       bigint primary key default poop.id_gen(),
        email    text unique not null,
        name     text not null,
        pw_salt    text not null,
        pw_hash    text not null,
        deleted boolean not null default false,
        created  timestamptz not null default now(),
        modified timestamptz not null default now()
    );
    create index users_email on poop.users(email);

    create table poop.auth_tokens (
        id bigint primary key default poop.id_gen(),
        user_id bigint not null references poop.users(id) on delete cascade,
        hash text unique not null,
        expires timestamptz not null,
        deleted boolean not null default false,
        created timestamptz not null default now(),
        modified timestamptz not null default now()
    );
    create index auth_tokens_user_id on poop.auth_tokens(user_id);
    create index auth_tokens_hash on poop.auth_tokens(hash);
    create index auth_tokens_expires on poop.auth_tokens(expires);
commit;
