param([string]$Dir)
$j = Get-Content "$Dir\response.json" -Raw -Encoding UTF8 | ConvertFrom-Json
"status=$($j.status) turn=$($j.turn_id) execution_failed=$($j.execution_failed)"
foreach($m in $j.messages){ $c=[string]$m.content; "[$($m.role)/$($m.kind)] " + $c.Substring(0,[Math]::Min(700,$c.Length)).Replace("`n"," | ") }
"notes: " + (($j.notes | ConvertTo-Json -Depth 3 -Compress) )