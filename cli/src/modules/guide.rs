use crate::imports::*;

#[derive(Default, Handler)]
#[help("Basic command guide for using this software.")]
pub struct Guide;

impl Guide {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> cli::Result<()> {
        let guide = include_str!("guide.txt");

        let lines = guide.split('\n');

        let mut paras = Vec::<String>::new();
        let mut para = String::new();
        let regex = Regex::new(r"\s+").unwrap();
        for line in lines {
            if line.trim().is_empty() {
                if !para.is_empty() {
                    let text = regex.replace_all(para.trim(), " ");
                    paras.push(text.to_string());
                    para.clear();
                }
            } else {
                para.push_str(line);
                para.push(' ');
            }
        }

        if !para.is_empty() {
            let regex = Regex::new(r"\s+").unwrap();
            let text = regex.replace_all(para.trim(), " ");
            paras.push(text.to_string());
            para.clear();
        }

        let desktop = Regex::new(r"^#(\[desktop\])?\s*").unwrap();

        for para in paras {
            if desktop.is_match(para.as_str()) {
                if !application_runtime::is_nw() {
                    continue;
                } else {
                    let text = desktop.replace(para.as_str(), "");
                    tprintln!(ctx);
                    tpara!(ctx, "{}", text);
                }
            } else {
                tprintln!(ctx);
                tpara!(ctx, "{}", para);
            }
        }
        tprintln!(ctx);

        Ok(())
    }
}
