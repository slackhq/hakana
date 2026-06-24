final class A {}
$a_class = rand(0, 1) !== 0 ? A::class : "blargle";
if (is_a($a_class, nameof A, true)) {
  echo "cool";
}
