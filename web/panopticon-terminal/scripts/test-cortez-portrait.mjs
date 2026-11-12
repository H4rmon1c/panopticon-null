import test from 'node:test';
import assert from 'node:assert/strict';
import {cortezPortraitFor, cortezPortraitImage, luceroPortrait} from '../cortez-portrait-api.js';

test('only the exact Cortez voter and meeting role receive the historical portrait', () => {
  assert.equal(cortezPortraitFor({name:'Orly Lucero',role:'Councilmember'}),luceroPortrait);
  assert.equal(cortezPortraitFor({name:'Orly Lucero',role:'Mayor'}),null);
  assert.equal(cortezPortraitFor({name:'Robert Dobry',role:'Councilmember'}).credit,'The Journal file');
  assert.equal(cortezPortraitFor({name:'Rachel Medina',role:'Mayor'}),null);
  assert.equal(cortezPortraitFor({name:'Rachel Medina',role:'Mayor Pro Tem'}).credit,'Courtesy photo, via The Journal');
  assert.match(luceroPortrait.source,/2017-Budget-in-Brief\?bidId=#page=2$/);
});

test('unavailable portrait removes its image element', () => {
  let failure, removed = false;
  const document = {createElement: tag => {
    assert.equal(tag, 'img');
    return {addEventListener: (event, handler) => { assert.equal(event, 'error'); failure = handler; },
      remove: () => { removed = true; }};
  }};
  const photo = cortezPortraitImage(document, {name:'Orly Lucero',role:'Councilmember'});
  assert.equal(photo.src, luceroPortrait.src);
  assert.equal(removed, false);
  failure();
  assert.equal(removed, true);
});
