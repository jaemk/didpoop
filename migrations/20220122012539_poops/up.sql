begin;
    create table poop.poops (
        id          bigint primary key default poop.id_gen(),
        creator_id  bigint not null references poop.users(id),
        creature_id bigint not null references poop.creatures(id),
        deleted     boolean not null default false,
        created     timestamptz not null default now(),
        modified    timestamptz not null default now()
    );
    create index idx_poop_creator on poop.poops(creator_id)
        where deleted is false;
    create index idx_poop_creature on poop.poops(creature_id)
        where deleted is false;
commit;
