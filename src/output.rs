

use lettre::{
    transport::smtp::authentication::Credentials, AsyncSmtpTransport, AsyncTransport, Message,
    Tokio1Executor,
};
use serde_json::Value;
use crate::{credentials::{ALERTS_EMAIL_PASSWORD, ALERTS_EMAIL_USERNAME}, settings::DEPLOY};

pub const SMTP_URL: &str = "mail.privateemail.com";
pub fn create_csv<T: serde::Serialize>(data: &Vec<T>, filename: &str) {
    let timestamp = chrono::Local::now().format("%F|%T").to_string();
    let filename = format!("{}{}-FundingRateSpreads.csv", filename, timestamp);
    let mut writer = csv::Writer::from_path(filename).expect("no path");
    for spread in data {
        writer.serialize(spread.clone()).unwrap();
    }
}

pub fn json<T:serde::Serialize>(filename: &str, contents: &T) {
    let filename = format!("{}.json", filename);
    serde_json::to_writer(&std::fs::File::create(filename).unwrap(), contents).unwrap();
}

pub fn delete_file(filename: &str) {
    std::fs::remove_file(filename).unwrap()
}

pub fn open_file(filename: &str) -> Result<Value, std::io::Error> {
    let file = std::fs::File::open(filename)?;
    let reader = std::io::BufReader::new(file);
    let data_file: Value = serde_json::from_reader(reader).unwrap();
    Ok(data_file)
}


pub async fn send_email(recipient_email: &str, email_body: &str, subject:&str) {
    let to = format!("Trader <{}>",recipient_email);
    let time = chrono::Utc::now();
    let pst_timestamp:String = time.with_timezone(&chrono_tz::US::Pacific).format("%F %T").to_string();
    let kst_timestamp:String = time.with_timezone(&chrono_tz::Asia::Seoul).format("%F %T").to_string();
    let stamped = format!("Times: \n{} KST.\n{} PST\n{}",kst_timestamp,pst_timestamp,email_body);
    let email = Message::builder()
        .from("Funding Rate Strategy <sender@tradequant.pro>".parse().unwrap())
        .to(to.parse().unwrap())
        .subject(subject)
        .body(stamped.to_string())
        .unwrap();

    let creds = Credentials::new(ALERTS_EMAIL_USERNAME.to_string(), ALERTS_EMAIL_PASSWORD.to_string());
    let mailer: AsyncSmtpTransport<Tokio1Executor> =
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(SMTP_URL)
            .unwrap()
            .credentials(creds)
            .build();
    if DEPLOY {
        match mailer.send(email).await {
            Ok(_) => println!("Email sent successfully"),
            Err(e) => println!("Could not send email: {:?}", e),
    }}
}