final class Builder {
 public function add(): this { return $this; }
}
function id<T>(T $v): T { return $v; }
function nested(Builder $b): Builder { return id($b->add()); }
function separate(Builder $b): Builder { $v=$b->add(); return id($v); }
