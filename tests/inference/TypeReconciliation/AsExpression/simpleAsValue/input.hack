final class Vehicle {}

function transport(mixed $m): Vehicle {
  $v = $m as Vehicle;
  return $v;
}