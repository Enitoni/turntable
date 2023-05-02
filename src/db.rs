use surrealdb::{
    engine::remote::ws::{Client, Ws},
    opt::auth::Root,
    Surreal,
};

pub type Database = Surreal<Client>;

pub async fn connect() -> Result<Database, surrealdb::Error> {
    let db = Surreal::new::<Ws>("127.0.0.1:8000").await?;

    db.signin(Root {
        username: "root",
        password: "root",
    })
    .await?;

    db.use_ns("vinyl").use_db("main").await?;

    Ok(db)
}
