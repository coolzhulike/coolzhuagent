function firstText(...values) {
  for (const value of values) {
    if (typeof value === 'string' && value.trim()) return value.trim();
  }
  return null;
}

function normalizedMentions(message) {
  const raw = message.mentions || message.mention_list || message.mentionList || [];
  if (!Array.isArray(raw)) return [];
  return [...new Set(raw.map((entry) => {
    if (typeof entry === 'string') return entry.trim();
    if (!entry || typeof entry !== 'object') return '';
    return firstText(
      entry.user_id,
      entry.userId,
      entry.open_id,
      entry.openId,
      entry.id,
    ) || '';
  }).filter(Boolean))];
}

export function normalizeIlinkGroupEvent(message, botAccountId) {
  const rawEvent = firstText(
    message.event_type,
    message.eventType,
    message.system_event,
    message.systemEvent,
    message.action,
  );
  if (!rawEvent) return null;
  const event = rawEvent.toLowerCase().replace(/[\s.-]+/g, '_');
  const normalized = [
    'bot_added',
    'bot_joined',
    'robot_added_to_group',
    'bot_join_group',
  ].includes(event)
    ? 'bot_added'
    : [
        'bot_removed',
        'bot_kicked',
        'robot_removed_from_group',
        'bot_left_group',
      ].includes(event)
      ? 'bot_removed'
      : null;
  if (!normalized) return null;

  const targetUserId = firstText(
    message.target_user_id,
    message.targetUserId,
    message.target_id,
    message.targetId,
    message.bot_user_id,
    message.botUserId,
  );
  if (targetUserId && botAccountId && targetUserId !== botAccountId) return null;
  return normalized;
}

export function normalizeIlinkInboundIdentity(message, botAccountId) {
  const groupId = firstText(
    message.conversation_id,
    message.conversationId,
    message.group_id,
    message.groupId,
    message.room_id,
    message.roomId,
    message.chat_id,
    message.chatId,
  );
  const fromUserId = firstText(
    message.from_user_id,
    message.fromUserId,
    message.user_id,
    message.userId,
    message.peer_id,
    message.peerId,
    message.sender,
  );
  const isGroup = Boolean(
    message.is_group
      || message.isGroup
      || groupId
      || message.chat_type === 'group'
      || message.chatType === 'group',
  );
  const peerId = (isGroup ? groupId : null) || fromUserId || 'unknown-peer';
  const senderId = isGroup
    ? firstText(
      message.sender_id,
      message.senderId,
      message.actual_sender_id,
      message.actualSenderId,
      message.member_id,
      message.memberId,
      fromUserId,
    )
    : fromUserId;
  const directName = firstText(
    message.from_user_name,
    message.fromUserName,
    message.nickname,
    message.nick_name,
    message.peer_name,
  );
  const senderName = isGroup
    ? firstText(
      message.sender_name,
      message.senderName,
      message.member_name,
      message.memberName,
      message.actual_sender_name,
      message.actualSenderName,
      directName,
    )
    : directName;
  const peerName = isGroup
    ? firstText(
      message.conversation_name,
      message.conversationName,
      message.group_name,
      message.groupName,
      message.room_name,
      message.roomName,
      message.peer_name,
    )
    : directName;
  const mentions = normalizedMentions(message);
  const mentionedBot = Boolean(
    message.mentioned_bot
      || message.mentionedBot
      || (botAccountId && mentions.includes(botAccountId)),
  );

  return {
    peerId,
    conversationId: isGroup ? (groupId || peerId) : null,
    peerName,
    senderId,
    senderName,
    isGroup,
    mentionedBot,
    mentions,
  };
}

export function summarizeIlinkInboundPayload(message) {
  const keys = Object.keys(message || {}).sort().slice(0, 64);
  const type = message?.message_type ?? message?.messageType ?? message?.type ?? null;
  return JSON.stringify({ keys, message_type: type });
}
