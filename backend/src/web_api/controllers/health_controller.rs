pub struct HealthController {}

impl HealthController {
    pub async fn get() -> String {
        let msg = "I'm up and running!";
        println!("{}", msg);
        msg.to_string()
    }
}