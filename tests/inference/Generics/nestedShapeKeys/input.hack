use namespace HH\Lib\{Dict, Keyset};
function keys(shape('a' => int, ?'b' => string) $data): keyset<string> {
  return Keyset\keys(Shapes::toDict($data));
}
function mapped(shape('a' => int, ?'b' => string) $data): dict<string,string> {
  return Dict\map_with_key(Shapes::toDict($data), ($k, $v) ==> (string)$v);
}
