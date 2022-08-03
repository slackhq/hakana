function foo(?shape('a' => int) $arr): int {
	return $arr is null ? 0 : $arr["a"]; 
}