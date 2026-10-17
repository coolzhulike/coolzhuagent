// 仅手动启用的真实 Windows 控件验收，目标必须来自本轮可见测试窗口。
#[cfg(windows)]
#[test]
#[ignore = "需要 COOLZHU_UIA_FIXTURE 指定已启动的可见测试窗口"]
fn real_uia_invoke_and_missing_target() {
    use computer_use::prepared_input::{NativeInputAuthorization, NativeInputPermit, PreparedNativeInput};
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct FixturePermit(AtomicUsize);
    impl NativeInputAuthorization for FixturePermit {
        fn authorize(&self, prepared: &PreparedNativeInput) -> Result<NativeInputPermit, String> {
            assert!(prepared.supervision_bound && prepared.process.pid>0);
            assert!(prepared.process.creation_time_filetime>0);
            Ok(NativeInputPermit {permit_id:"uia-fixture-permit".into(),attempt_id:"uia-fixture-action".into(),executor_instance_id:"uia-fixture-executor".into(),expires_at_unix_ms:prepared.deadline_unix_ms})
        }
        fn dispatch(&self,_:&PreparedNativeInput,_:&NativeInputPermit,notify:&mut dyn FnMut()->Result<(),String>)->Result<(),String>{
            self.0.fetch_add(1,Ordering::SeqCst);notify()
        }
    }
    let info=std::path::PathBuf::from(std::env::var_os("COOLZHU_UIA_FIXTURE").expect("真实窗口收据"));
    let fixture:JsonValue=serde_json::from_slice(&std::fs::read(&info).unwrap()).unwrap();
    let hwnd=fixture["hwnd"].as_i64().unwrap() as isize;
    uia_resolver::focus_window(hwnd).unwrap();
    let actual=uia_resolver::snapshot_foreground_window(200).unwrap();
    assert_eq!(actual.native_window_handle,hwnd);
    assert_eq!(u64::from(actual.process_id),fixture["pid"].as_u64().unwrap());
    let button=actual.elements.iter().find(|e|e.name.as_deref()==Some("UIA-ACCEPT-INVOKE")).expect("可见按钮");
    assert!(button.patterns.iter().any(|p|p=="invoke"));
    assert!(button.runtime_id.as_ref().is_some_and(|v|!v.is_empty()));
    let snapshot=DesktopSnapshot {
        window_id:format!("hwnd-{hwnd}"),process_id:actual.process_id,
        window_rect:[actual.bounding_rect.x,actual.bounding_rect.y,actual.bounding_rect.width,actual.bounding_rect.height],dpi:actual.dpi,
        webview2_overlay:false,state:json!({"window":{"native_window_handle":hwnd},"elements":[element_json(button)]}),evidence:vec![],
    };
    let action=ComputerUseAction {kind:ComputerUseActionKind::Click,target:button.reference.clone(),arguments:json!({}),risk:computer_use::ComputerUseRiskClass::ReversibleLocal};
    let port=FixturePermit(AtomicUsize::new(0));
    let bridge=DesktopNativeBridge::default();
    let positive=bridge.execute_authorized(&action,&snapshot,Duration::from_secs(25),&port).unwrap();
    assert!(positive.summary.contains("uia_invoke"),"不能退回坐标点击：{positive:?}");
    assert_eq!(positive.receipt.as_ref().unwrap().input_delivery,InputDelivery::Sent);
    let counter=info.parent().unwrap().join("counter.txt");
    let until=std::time::Instant::now()+Duration::from_secs(3);
    while std::fs::read_to_string(&counter).unwrap_or_default()!="1" && std::time::Instant::now()<until {std::thread::sleep(Duration::from_millis(50));}
    assert_eq!(std::fs::read_to_string(&counter).unwrap(),"1");
    let mut missing=snapshot.clone();
    missing.state["elements"][0]["runtime_id"]=json!([42,-2147483647]);
    let rejected=bridge.execute_authorized(&action,&missing,Duration::from_secs(25),&port).expect_err("过期控件不可回退点击");
    assert_eq!(rejected.receipt().unwrap().input_delivery,InputDelivery::NotSent);
    assert_eq!(std::fs::read_to_string(&counter).unwrap(),"1");
    assert_eq!(port.0.load(Ordering::SeqCst),2);
    let fresh=uia_resolver::snapshot_foreground_window(200).unwrap();
    assert!(fresh.elements.iter().any(|e|e.name.as_deref()==Some("INVOKED-1")));
    std::fs::write(info.parent().unwrap().join("result.json"),serde_json::to_vec_pretty(&json!({
        "production_bridge":true,"production_supervised_helper":true,"test_authorization_port":true,
        "window_pid":actual.process_id,"runtime_id":button.runtime_id,"invoke_count":1,
        "missing_runtime_id_not_sent":true,"no_coordinate_fallback":true,"ui_label":"INVOKED-1",
        "positive_summary":positive.summary,"negative_code":rejected.code,
    })).unwrap()).unwrap();
}
