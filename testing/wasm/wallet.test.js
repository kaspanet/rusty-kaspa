const kaspa = require('./kaspa')
const assert = require('assert')

const testData = require('./src/testData.json')

describe ('handling wallets', () => {
  it ('handles addresses', () => {
    const validAddress = new kaspa.Address(testData.wallet.address.valid)
    //TODO: Do assert test for validAddress when hex fixed

    const invalidAddresses = Object.values(testData.wallet.address.invalid)
    let detectedInvalids = 0

    invalidAddresses.forEach(address => {
      try {
        new kaspa.Address(address)
      } catch {
        detectedInvalids += 1
      }
    })

    assert.strictEqual(invalidAddresses.length, detectedInvalids, `Theres ${invalidAddresses.length} invalid addresses but only detected ${detectedInvalids}`)
  })

  it ('creates and handles mnemonics', () => {
    
  })
})
