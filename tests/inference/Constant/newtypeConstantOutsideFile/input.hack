function make_message(): message_t {
	return shape(
		'user_id' => SLACKBOT_USER_ID,
		'text' => 'hello',
	);
}

final class Encoding {
	const decoded_id_t CLASS_USER_ID = 2;
}

function make_message2(): message_t {
	return shape(
		'user_id' => Encoding::CLASS_USER_ID,
		'text' => 'hello',
	);
}

const int PLAIN_INT = 5;

function use_plain(): int {
	return PLAIN_INT;
}
