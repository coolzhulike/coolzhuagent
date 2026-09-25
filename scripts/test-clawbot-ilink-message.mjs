import assert from 'node:assert/strict';

import {
  normalizeIlinkGroupEvent,
  normalizeIlinkInboundIdentity,
  summarizeIlinkInboundPayload,
} from './lib/clawbot-ilink-message.mjs';

const direct = normalizeIlinkInboundIdentity({
  from_user_id: 'peer-direct',
  from_user_name: '联系人',
}, 'bot-main');
assert.deepEqual(direct, {
  peerId: 'peer-direct',
  conversationId: null,
  peerName: '联系人',
  senderId: 'peer-direct',
  senderName: '联系人',
  isGroup: false,
  mentionedBot: false,
  mentions: [],
});

const group = normalizeIlinkInboundIdentity({
  conversation_id: 'group-1',
  conversation_name: '项目群',
  from_user_id: 'group-1',
  sender_id: 'member-1',
  sender_name: '测试成员',
  is_group: true,
  mentions: [{ user_id: 'bot-main' }, { user_id: 'member-2' }],
}, 'bot-main');
assert.deepEqual(group, {
  peerId: 'group-1',
  conversationId: 'group-1',
  peerName: '项目群',
  senderId: 'member-1',
  senderName: '测试成员',
  isGroup: true,
  mentionedBot: true,
  mentions: ['bot-main', 'member-2'],
});

const officialGroup = normalizeIlinkInboundIdentity({
  group_id: 'group-official',
  group_name: '官方字段群',
  from_user_id: 'member-official',
  from_user_name: '官方字段成员',
  is_group: true,
  mentions: [{ user_id: 'bot-main' }],
}, 'bot-main');
assert.deepEqual(officialGroup, {
  peerId: 'group-official',
  conversationId: 'group-official',
  peerName: '官方字段群',
  senderId: 'member-official',
  senderName: '官方字段成员',
  isGroup: true,
  mentionedBot: true,
  mentions: ['bot-main'],
});

assert.equal(normalizeIlinkGroupEvent({
  group_id: 'group-official',
  event_type: 'bot_removed',
  target_user_id: 'bot-main',
}, 'bot-main'), 'bot_removed');
assert.equal(normalizeIlinkGroupEvent({
  group_id: 'group-official',
  event_type: 'bot_removed',
  target_user_id: 'another-bot',
}, 'bot-main'), null);
assert.equal(normalizeIlinkGroupEvent({
  group_id: 'group-official',
  event_type: 'member_joined',
  target_user_id: 'bot-main',
}, 'bot-main'), null);

const summary = summarizeIlinkInboundPayload({
  conversation_id: 'group-1',
  sender_id: 'member-1',
  text: '不应进入诊断摘要的敏感正文',
  message_type: 2,
});
assert.match(summary, /conversation_id/);
assert.match(summary, /sender_id/);
assert.match(summary, /message_type/);
assert.doesNotMatch(summary, /敏感正文/);

console.log('iLink inbound identity normalization: PASS');
