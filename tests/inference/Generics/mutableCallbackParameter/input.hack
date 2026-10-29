final class Cache<TSuffix as arraykey> {
    public function __construct(private dict<TSuffix, int> $hot_keys) {}
    public function fill<T>(TSuffix $key, (function(TSuffix): T) $fill): T {
        return $fill($key);
    }
    public function ttl(TSuffix $key): int {
        return $this->hot_keys[$key] ?? 0;
    }
}
function lookup(string $key): int {
    $cache = new Cache(dict['hot' => 100]);
    $cache->fill('to_fill', $_ ==> 1);
    return $cache->ttl($key);
}
