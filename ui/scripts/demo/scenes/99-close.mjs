// The closing card. Worth its own file rather than a tail on the last scene, because which
// scene is last changes every time one is added.
export default {
  id: '99-close',
  title: 'Closing',

  async run(s) {
    await s.say('Brave Bot', 'Three columns, five questions, and no key that answers one of them.', 3)
    await s.say('Brave Bot', 'Sessions you come back to, and bots that remember why.', 2.6)
    await s.say('Brave Bot', 'github.com/brave-experiments/brave-bot-ui', 3)
    await s.shot('99-close')
  },
}
