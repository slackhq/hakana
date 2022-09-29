function _chat_channels_format_multi_join(
	vec<shape('id' => int)> $users,
): shape('user_ids' => vec<int>) {
	$arr = shape(
		'user_ids' => vec[],
	);

	foreach ($users as $user) {
		$arr['user_ids'][] = $user['id'];
	}

	return $arr;
}