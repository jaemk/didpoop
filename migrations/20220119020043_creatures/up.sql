begin;
    create table poop.creatures (
        id         bigint primary key default poop.id_gen(),
        creator_id bigint not null references poop.users(id),
        name       text not null,
        deleted    boolean not null default false,
        created    timestamptz not null default now(),
        modified   timestamptz not null default now()
    );
    create index idx_creatures_creator on poop.creatures(creator_id)
        where deleted is false;

    create table poop.creature_access_kind (
        kind text primary key
    );
    insert into poop.creature_access_kind (kind) values
        ('creator'),
        ('pooper'),
        ('reader');

    create table poop.creature_access (
        id          bigint primary key default poop.id_gen(),
        creature_id bigint not null references poop.creatures(id),
        user_id     bigint not null references poop.users(id),
        creator_id  bigint not null references poop.users(id),
        kind        text not null references poop.creature_access_kind(kind),
        deleted     boolean not null default false,
        created     timestamptz not null default now(),
        modified    timestamptz not null default now()
    );
    create index idx_creature_access_creature on poop.creature_access(creature_id)
        where deleted is false;
    create index idx_creature_access_user on poop.creature_access(user_id)
        where deleted is false;
    create index idx_creature_access_creator on poop.creature_access(creator_id)
        where deleted is false;
    create index idx_creature_access_kind on poop.creature_access(kind)
        where deleted is false;
commit;
