use serde::{Deserialize, Serialize};
use worker::*;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ClientRow {
    pub id: String,
    pub name: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub created_at: i64,
    pub disabled: i64,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct RouteRow {
    pub id: String,
    pub client_id: String,
    pub kind: String,
    pub matcher: String,
    pub target: String,
    pub strip_prefix: i64,
    pub created_at: i64,
}

pub async fn insert_client(db: &D1Database, c: &ClientRow) -> Result<()> {
    db.prepare("INSERT INTO clients (id,name,token_hash,token_prefix,created_at,disabled) VALUES (?1,?2,?3,?4,?5,?6)")
        .bind(&[
            c.id.clone().into(),
            c.name.clone().into(),
            c.token_hash.clone().into(),
            c.token_prefix.clone().into(),
            (c.created_at as f64).into(),
            (c.disabled as f64).into(),
        ])?
        .run()
        .await?;
    Ok(())
}

pub async fn list_clients(db: &D1Database) -> Result<Vec<ClientRow>> {
    let res = db
        .prepare("SELECT * FROM clients ORDER BY created_at DESC")
        .all()
        .await?;
    res.results::<ClientRow>()
}

pub async fn find_client_by_token_hash(db: &D1Database, hash: &str) -> Result<Option<ClientRow>> {
    db.prepare("SELECT * FROM clients WHERE token_hash = ?1 AND disabled = 0")
        .bind(&[hash.into()])?
        .first::<ClientRow>(None)
        .await
}

pub async fn set_client_disabled(db: &D1Database, id: &str, disabled: bool) -> Result<()> {
    db.prepare("UPDATE clients SET disabled = ?1 WHERE id = ?2")
        .bind(&[(disabled as i64 as f64).into(), id.into()])?
        .run()
        .await?;
    Ok(())
}

pub async fn delete_client(db: &D1Database, id: &str) -> Result<()> {
    db.prepare("DELETE FROM clients WHERE id = ?1")
        .bind(&[id.into()])?
        .run()
        .await?;
    Ok(())
}

pub async fn insert_route(db: &D1Database, r: &RouteRow) -> Result<()> {
    db.prepare("INSERT INTO routes (id,client_id,kind,matcher,target,strip_prefix,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)")
        .bind(&[
            r.id.clone().into(),
            r.client_id.clone().into(),
            r.kind.clone().into(),
            r.matcher.clone().into(),
            r.target.clone().into(),
            (r.strip_prefix as f64).into(),
            (r.created_at as f64).into(),
        ])?
        .run()
        .await?;
    Ok(())
}

pub async fn list_routes(db: &D1Database) -> Result<Vec<RouteRow>> {
    let res = db
        .prepare("SELECT * FROM routes ORDER BY created_at DESC")
        .all()
        .await?;
    res.results::<RouteRow>()
}

pub async fn find_route(db: &D1Database, kind: &str, matcher: &str) -> Result<Option<RouteRow>> {
    db.prepare("SELECT * FROM routes WHERE kind = ?1 AND matcher = ?2")
        .bind(&[kind.into(), matcher.into()])?
        .first::<RouteRow>(None)
        .await
}

pub async fn delete_route(db: &D1Database, id: &str) -> Result<()> {
    db.prepare("DELETE FROM routes WHERE id = ?1")
        .bind(&[id.into()])?
        .run()
        .await?;
    Ok(())
}
