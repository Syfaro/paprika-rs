CREATE TABLE status (
    name TEXT PRIMARY KEY,
    position INTEGER NOT NULL
);

CREATE TABLE recipe (
    id SERIAL PRIMARY KEY,
    cook_time TEXT,
    created TIMESTAMP WITH TIME ZONE NOT NULL,
    description TEXT,
    difficulty TEXT,
    directions TEXT NOT NULL,
    hash TEXT NOT NULL,
    image_url TEXT,
    in_trash BOOLEAN NOT NULL,
    ingredients TEXT NOT NULL,
    is_pinned BOOLEAN NOT NULL,
    name TEXT NOT NULL,
    notes TEXT NOT NULL,
    on_favorites BOOLEAN NOT NULL,
    on_grocery_list BOOLEAN NOT NULL,
    photo TEXT,
    photo_hash TEXT,
    photo_large TEXT,
    photo_url TEXT,
    prep_time TEXT,
    rating INTEGER NOT NULL,
    scale TEXT,
    servings TEXT,
    source TEXT,
    source_url TEXT,
    total_time TEXT,
    uid TEXT UNIQUE NOT NULL
);

CREATE TABLE meal (
    id SERIAL PRIMARY KEY,
    uid TEXT UNIQUE NOT NULL,
    recipe_uid TEXT NOT NULL,
    date TIMESTAMP WITH TIME ZONE NOT NULL,
    meal_type INTEGER NOT NULL,
    name TEXT NOT NULL,
    order_flag INTEGER NOT NULL,
    type_uid TEXT NOT NULL
);

CREATE TABLE grocery_item (
    id SERIAL PRIMARY KEY,
    uid TEXT UNIQUE NOT NULL,
    recipe_uid TEXT,
    name TEXT NOT NULL,
    order_flag INTEGER NOT NULL,
    purchased BOOLEAN NOT NULL,
    aisle TEXT NOT NULL,
    ingredient TEXT NOT NULL,
    recipe TEXT,
    instruction TEXT NOT NULL,
    quantity TEXT NOT NULL,
    separate BOOLEAN NOT NULL,
    aisle_uid TEXT NOT NULL,
    list_uid TEXT NOT NULL
);

CREATE TABLE aisle (
    id SERIAL PRIMARY KEY,
    uid TEXT UNIQUE NOT NULL,
    name TEXT UNIQUE NOT NULL,
    order_flag INTEGER NOT NULL
);

CREATE TABLE menu (
    id SERIAL PRIMARY KEY,
    uid TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    notes TEXT NOT NULL,
    order_flag INTEGER NOT NULL,
    days INTEGER NOT NULL
);

CREATE TABLE menu_item (
    id SERIAL PRIMARY KEY,
    uid TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    order_flag INTEGER NOT NULL,
    recipe_uid TEXT NOT NULL,
    menu_uid TEXT NOT NULL,
    type_uid TEXT NOT NULL,
    day INTEGER NOT NULL
);

CREATE TABLE photo (
    id SERIAL PRIMARY KEY,
    uid TEXT UNIQUE NOT NULL,
    filename TEXT NOT NULL,
    recipe_uid TEXT NOT NULL,
    order_flag INTEGER NOT NULL,
    name TEXT NOT NULL,
    hash TEXT NOT NULL
);

CREATE TABLE meal_type (
    id SERIAL PRIMARY KEY,
    uid TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    order_flag INTEGER NOT NULL,
    color TEXT NOT NULL,
    export_all_day BOOLEAN NOT NULL,
    export_time INTEGER NOT NULL,
    original_type INTEGER NOT NULL
);

CREATE TABLE pantry_item (
    id SERIAL PRIMARY KEY,
    uid TEXT UNIQUE NOT NULL,
    ingredient TEXT NOT NULL,
    aisle TEXT NOT NULL,
    expiration_date TIMESTAMP WITH TIME ZONE,
    has_expiration BOOLEAN NOT NULL,
    in_stock BOOLEAN NOT NULL,
    purchase_date TIMESTAMP WITH TIME ZONE NOT NULL,
    quantity TEXT NOT NULL,
    aisle_uid TEXT NOT NULL
);

CREATE TABLE grocery_ingredient (
    id SERIAL PRIMARY KEY,
    uid TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    aisle_uid TEXT
);

CREATE TABLE grocery_list (
    id SERIAL PRIMARY KEY,
    uid TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    order_flag INTEGER NOT NULL,
    is_default BOOLEAN NOT NULL,
    reminders_list TEXT NOT NULL
);

CREATE TABLE bookmark (
    id SERIAL PRIMARY KEY,
    uid TEXT UNIQUE NOT NULL,
    title TEXT NOT NULL,
    url TEXT NOT NULL,
    order_flag INTEGER NOT NULL
);

CREATE TABLE category (
    id SERIAL PRIMARY KEY,
    uid TEXT UNIQUE NOT NULL,
    order_flag INTEGER NOT NULL,
    name TEXT NOT NULL,
    parent_uid TEXT
);
