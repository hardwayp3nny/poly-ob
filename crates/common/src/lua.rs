pub const LUA_CAS_UPDATE: &str = r#"
-- KEYS[1]=ob:{token_id}
-- ARGV: new_hash, new_ts, bids_json, asks_json, now_ms, market
local h = redis.call('HGETALL', KEYS[1])
local cur = {}
for i=1,#h,2 do cur[h[i]] = h[i+1] end
if cur['hash'] == ARGV[1] then return 'skip_hash' end
local cur_ts = tonumber(cur['timestamp'] or '0')
local new_ts = tonumber(ARGV[2])
if new_ts < cur_ts then return 'skip_ts' end
redis.call('HMSET', KEYS[1],
  'hash', ARGV[1], 'timestamp', ARGV[2],
  'bids', ARGV[3], 'asks', ARGV[4], 'updated_at', ARGV[5], 'market', ARGV[6]
)
return 'updated'
"#;


