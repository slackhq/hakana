function get_nonnull<Tu, T as ?Tu>(T $t): Tu {
    if ($t is null) {
      throw new \Exception('bad');
    }
    return $t;
}

function foo(?string $a): string {
  $a = get_nonnull($a);
  return $a;
}