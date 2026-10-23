param(
  [Parameter(Mandatory)][ValidateSet('baseline','candidate')][string]$Side,
  [Parameter(Mandatory)][string]$Case,
  [Parameter(Mandatory)][int]$Round,
  [Parameter(Mandatory)][string]$PromptFile,
  [string]$Session = 'session-1792519612697-22ffea3a2956ec99',
  [switch]$ResetScenario,
  [string]$ScenarioDir = 'D:\coolzhuagent\tmp\ha11-scenario',
  [string]$FollowUpFile = '',
  [int]$TimeoutSec = 900
)
$ErrorActionPreference = 'Stop'
$root = 'D:\coolzhuagent\tmp\ha11-run'
$port = if ($Side -eq 'baseline') { 8766 } else { 8767 }
$base = "http://127.0.0.1:$port"
$ws = "$root\$Side\ws"
$out = "D:\coolzhuagent\docs\plans\harness-alignment\evidence\HA-11\20261021\runs\$Case-$Side-$Round"
New-Item -ItemType Directory -Force $out | Out-Null
if ($ResetScenario) {
  foreach ($n in 'src','tests','target','Cargo.toml','Cargo.lock') { Remove-Item "$ws\$n" -Recurse -Force -ErrorAction SilentlyContinue }
  Copy-Item "$ScenarioDir\*" $ws -Recurse -Force
}
$prompt = [IO.File]::ReadAllText($PromptFile, [Text.Encoding]::UTF8).Trim()
function J($o) { ,[Text.Encoding]::UTF8.GetBytes(($o | ConvertTo-Json -Depth 8 -Compress)) }
foreach ($r in (Invoke-RestMethod "$base/api/chat/rooms").rooms) { if ($r.name -like "ha11-*") { Invoke-RestMethod "$base/api/chat/rooms/$($r.id)" -Method Delete | Out-Null } }
$room = Invoke-RestMethod "$base/api/chat/rooms" -Method Post -ContentType 'application/json; charset=utf-8' -Body (J @{ name = "ha11-$Case-$Side-$Round" })
$room | ConvertTo-Json -Depth 6 | Out-File "$out\room.json" -Encoding utf8
$roomId = if ($room.room) { $room.room.id } elseif ($room.id) { $room.id } else { $room.chat_room_id }
$body = J @{ session_id = $Session; chat_room_id = $roomId; target_agent_ids = @($Session); text = $prompt }
$sw = [Diagnostics.Stopwatch]::StartNew()
$resp = Invoke-WebRequest "$base/api/chat/send" -Method Post -ContentType 'application/json; charset=utf-8' -Body $body -UseBasicParsing -TimeoutSec $TimeoutSec
$sw.Stop()
$firstEnd = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$bytes = $resp.RawContentStream.ToArray()
[IO.File]::WriteAllText("$out\response.json", [Text.Encoding]::UTF8.GetString($bytes), (New-Object Text.UTF8Encoding $false))
@{ side = $Side; case = $Case; round = $Round; room_id = $roomId; http = [int]$resp.StatusCode; elapsed_ms = $sw.ElapsedMilliseconds; first_turn_end_ms = $firstEnd; prompt = $prompt } | ConvertTo-Json | Out-File "$out\meta.json" -Encoding utf8
if ($FollowUpFile) {
  $p2 = [IO.File]::ReadAllText($FollowUpFile, [Text.Encoding]::UTF8).Trim()
  $sw2 = [Diagnostics.Stopwatch]::StartNew()
  $resp2 = Invoke-WebRequest "$base/api/chat/send" -Method Post -ContentType 'application/json; charset=utf-8' -Body (J @{ session_id = $Session; chat_room_id = $roomId; target_agent_ids = @($Session); text = $p2 }) -UseBasicParsing -TimeoutSec $TimeoutSec
  $sw2.Stop()
  [IO.File]::WriteAllText("$out\response2.json", [Text.Encoding]::UTF8.GetString($resp2.RawContentStream.ToArray()), (New-Object Text.UTF8Encoding $false))
  @{ followup = $p2; elapsed_ms = $sw2.ElapsedMilliseconds; http = [int]$resp2.StatusCode; second_turn_start_ms = $firstEnd } | ConvertTo-Json | Out-File "$out\meta2.json" -Encoding utf8
}
"elapsed_ms=$($sw.ElapsedMilliseconds) http=$($resp.StatusCode) room=$roomId out=$out"