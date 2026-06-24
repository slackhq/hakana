$e = rand(0, 10) !== 0
  ? new RuntimeException("m")
  : null;

if ($e is Exception) {
  echo "good";
}