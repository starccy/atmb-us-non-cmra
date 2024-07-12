# atmb-us-non-cmra

优选 [anytimemailbox](https://www.anytimemailbox.com/) 地址。

## 功能

结合 [第三方](https://www.smarty.com/) 查询接口，过滤出非 CMRA 的地址。
并按照是否为住宅地址进行排序。

运行结果保存为 csv 文件，可以在 [这里](./result/mailboxes.csv) 查看。


## 本地运行

1. 安装 [rust](https://www.rust-lang.org/) 环境，并确保使用 nightly 版本，且版本号在 1.80 或以上。
2. 注册 [smarty](https://www.smarty.com/) 帐号，并获取 API key. 由于免费帐号一个月只能查询 1000 次，而 atmb 目前有 1700 多个美国地址，所以至少需要注册两个帐号
    来完成查询。
3. 设置环境变量 `CRENDENTIALS`, 值的格式为：
    `API_ID1=API_TOKEN1,API_ID2=API_TOKEN2`
    将 `API_ID1`、`API_TOKEN1` 等替换为实际的 API ID 和 TOKEN。
4. 进入项目根目录，命令行执行 `cargo run --release`。
5. 等待程序运行完成，查看运行结果： `result/mailboxes.csv`。

## TODO
使用 Github Action 定时更新地址列表
