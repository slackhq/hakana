function reduce<Tv, Ta>(
  Traversable<Tv> $_,
  (function(Ta, Tv): Ta) $_,
  Ta $initial,
): Ta {
  return $initial;
}

function test(vec<int> $trav): vec<int> {
  return reduce($trav, ($acc_v, $element) ==> {
    $acc_v[] = $element;
    return $acc_v;
  }, vec[]);
}
