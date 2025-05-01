// @generated automatically by Diesel CLI.

diesel::table! {
    guilds (id) {
        id -> Int8,
        prefix -> Nullable<Varchar>,
    }
}

diesel::table! {
    users (id) {
        id -> Int8,
        ign -> Nullable<Varchar>,
        uuid -> Nullable<Varchar>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(guilds, users,);
