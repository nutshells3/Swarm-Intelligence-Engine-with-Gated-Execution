-- Peer messages between agents
create table if not exists peer_messages (
    message_id text primary key,
    from_task_id text not null references tasks(task_id),
    to_task_id text references tasks(task_id),  -- null = broadcast
    topic text not null,
    kind text not null,
    payload jsonb not null default '{}'::jsonb,
    created_at timestamptz not null default now()
);

create index if not exists idx_peer_messages_topic on peer_messages(topic);
create index if not exists idx_peer_messages_to_task on peer_messages(to_task_id);
create index if not exists idx_peer_messages_from_task on peer_messages(from_task_id);
create index if not exists idx_peer_messages_created on peer_messages(created_at);

-- Peer message acknowledgements
create table if not exists peer_message_acks (
    ack_id text primary key,
    message_id text not null references peer_messages(message_id),
    acknowledged_by text not null,
    response jsonb,
    created_at timestamptz not null default now()
);

create index if not exists idx_peer_acks_message on peer_message_acks(message_id);

-- Topic subscriptions (which task listens to what)
create table if not exists peer_subscriptions (
    subscription_id text primary key,
    subscriber_task_id text not null references tasks(task_id),
    topic text not null,
    created_at timestamptz not null default now(),
    unique(subscriber_task_id, topic)
);

create index if not exists idx_peer_subs_topic on peer_subscriptions(topic);
