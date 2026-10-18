import test from 'node:test';
import assert from 'node:assert/strict';
// Reference only: shared retry budget across long offline partitions.
class DeadlinePlan {
 constructor(base=100,cap=1600,deadline=5000) {
  if(base<1||base>cap||deadline<1)throw Error('invalid_budget');
  this.base=base;this.cap=cap;this.deadline=deadline;
  this.pending=new Map();this.acked=new Map();
 }
 stage(key,digest) {
  if(!key||! /^[a-f0-9]{64}$/.test(digest))throw Error('invalid_effect');
  if(this.pending.has(key)&&this.pending.get(key)!==digest)throw Error('digest_conflict');
  if(this.acked.has(key)&&this.acked.get(key)!==digest)throw Error('digest_conflict');
  if(!this.acked.has(key))this.pending.set(key,digest);
 }
 schedule(maxAttempts) {
  let elapsed=0;const times=[];
  for(let n=0;n<maxAttempts;n++) {
   const delay=Math.min(this.cap,this.base*2**Math.min(20,n));
   if(elapsed+delay>this.deadline)break;
   times.push(elapsed+=delay);
  }
  return times;
 }
 ack(key,digest) {
  if(this.pending.get(key)!==digest)throw Error('ack_conflict');
  this.pending.delete(key);this.acked.set(key,digest);
 }
}
test('one end-to-end deadline bounds repeated failures',()=>{
 assert.deepEqual(new DeadlinePlan(100,800,1000).schedule(40),[100,300,700]);
});
test('lost response replay preserves exact logical effect',()=>{
 const plan=new DeadlinePlan();plan.stage('e','a'.repeat(64));
 plan.stage('e','a'.repeat(64));plan.ack('e','a'.repeat(64));
 assert.equal(plan.acked.size,1);
 assert.throws(()=>plan.stage('e','b'.repeat(64)),/digest_conflict/);
});
test('wrong ack cannot silently clear the pending queue',()=>{
 const plan=new DeadlinePlan();plan.stage('e','a'.repeat(64));
 assert.throws(()=>plan.ack('e','b'.repeat(64)),/ack_conflict/);
 assert.equal(plan.pending.size,1);
});
