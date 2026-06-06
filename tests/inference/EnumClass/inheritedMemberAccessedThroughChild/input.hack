enum class ParentStatus: string {
	string ACCEPTED = 'accepted';
	string DECLINED = 'declined';
}

enum class ChildStatus: string extends ParentStatus {
	string NEEDS_ACTION = 'needsAction';
}

function parse_status(string $s): HH\MemberOf<ChildStatus, string> {
	switch ($s) {
		case 'accepted':
			return ChildStatus::ACCEPTED;
		case 'needsAction':
			return ChildStatus::NEEDS_ACTION;
		default:
			return ChildStatus::DECLINED;
	}
}
