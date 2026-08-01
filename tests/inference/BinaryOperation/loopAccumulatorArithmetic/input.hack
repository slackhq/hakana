function sum_warnings(vec<dict<int, int>> $tasks): dict<int, int> {
	$out = dict[];
	foreach ($tasks as $t) {
		foreach ($t as $k => $v) {
			if (\HH\Lib\C\contains_key($out, $k)) {
				$out[$k] += $v;
			} else {
				$out[$k] = $v;
			}
		}
	}
	return $out;
}

function count_warnings(vec<int> $ids): dict<int, int> {
	$out = dict[];
	foreach ($ids as $id) {
		if (\HH\Lib\C\contains_key($out, $id)) {
			$out[$id]++;
		} else {
			$out[$id] = 1;
		}
	}
	return $out;
}

function overflowing_literal_int(): int {
	return 9223372036854775807 + 1;
}

// Shapes::idx on an absent optional field yields nothing|default;
// arithmetic on it stays int
type chan_t = shape('num' => int, ?'num_vip' => int);

function accum_shapes_idx(vec<int> $rows): dict<int, chan_t> {
	$by_channel = dict[];
	foreach ($rows as $r) {
		$channel_data = $by_channel[$r] ?? shape('num' => 0);
		$channel_data['num'] = $channel_data['num'] + 1;
		if ($r > 5) {
			$channel_data['num_vip'] = Shapes::idx($channel_data, 'num_vip', 0) + 1;
		}
		$by_channel[$r] = $channel_data;
	}
	return $by_channel;
}
