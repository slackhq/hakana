const K = 5;

abstract class C6 {

  const dict<int, int> M = dict[
    1 => -1,
    K => 6,
  ];

  public static function f(int $k): void {
      $a = self::M;
      print_r($a);
  }

}