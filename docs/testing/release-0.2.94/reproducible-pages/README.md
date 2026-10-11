# 真实网页复现说明

这些脚本只提供受控网页和可信输入事件采集，不模拟模型、插件结果或宿主回执。需使用正式安装的CoolzhuAgent、既有真实模型会话和当前右栏原生浏览器完成动作。事件记录中的trusted须为true；业务结果、CU宿主终态、实际截图和远端正常收尾必须分别核对。

```powershell
python docs/testing/release-0.2.94/reproducible-pages/network-navigation-server.py --output-dir tmp/reproduce094-network
python docs/testing/release-0.2.94/reproducible-pages/integration-order-server.py --output-dir tmp/reproduce094-order
```

每个脚本占一个终端。读取各输出目录的address.json，分别打开/before.html或/order.html。生成stop-server文件可正常停止对应服务器。输出目录须为新的tmp子目录，不要写入证据归档目录。Python只需要标准库。

网络页将127.0.0.1切换为localhost；这是真实跨来源HTTP导航。对比page_ms顺序，准确区分导航发起、beforeunload、释放和新文档load；没有命中commit发生于按下期间时，不声称覆盖这一窄时序。

订单页使用真实DSH计算器求42*19+17，再在同一CU任务中填写、提交并根据网页反馈继续。不要把内部配送逻辑预告给模型。通过需要真实计算器单次调用台账、可信business-rejected→delivery-confirmed→order-completed事件、最终结算单和总价、CU成功与绑定解锁，不能只采用模型自述。网页记录的是本机合成采购数据，不连接支付服务。
