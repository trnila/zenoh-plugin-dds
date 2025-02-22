//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//

#![feature(vec_into_raw_parts)]

pub mod coders;
pub mod gst_coder;

use cyclors::*;
use log::debug;
use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::os::raw;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use zenoh::net::{ResKey, Session};

use crate::coders::{Coder, Coders, ZenohWriter};



const MAX_SAMPLES: usize = 32;

#[derive(Debug)]
pub struct QosHolder(pub *mut dds_qos_t);
// unsafe impl Send for QosHolder {}
// unsafe impl Sync for QosHolder {}

#[derive(Debug)]
pub enum MatchedEntity {
    DiscoveredPublication {
        topic_name: String,
        type_name: String,
        keyless: bool,
        partition: Option<String>,
        qos: QosHolder,
    },
    UndiscoveredPublication {
        topic_name: String,
        type_name: String,
        partition: Option<String>,
    },
    DiscoveredSubscription {
        topic_name: String,
        type_name: String,
        keyless: bool,
        partition: Option<String>,
        qos: QosHolder,
    },
    UndiscoveredSubscription {
        topic_name: String,
        type_name: String,
        partition: Option<String>,
    },
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn print_qos_partitions(qos: *const dds_qos_t) {
    let mut n: u32 = 0;
    let mut ps: *mut *mut ::std::os::raw::c_char = std::ptr::null_mut();
    unsafe {
        let _ = dds_qget_partition(
            qos,
            &mut n as *mut u32,
            &mut ps as *mut *mut *mut ::std::os::raw::c_char,
        );
        if n > 0 {
            for k in 0..n {
                let p = CStr::from_ptr(*(ps.offset(k as isize))).to_str().unwrap();
                debug!("Partition[{}]: {:?}", k, p);
            }
        }
    }
}

unsafe extern "C" fn on_data(dr: dds_entity_t, arg: *mut std::os::raw::c_void) {
    let btx = Box::from_raw(arg as *mut (bool, Sender<MatchedEntity>));
    let dp = dds_get_participant(dr);
    let mut dpih: dds_instance_handle_t = 0;
    let _ = dds_get_instance_handle(dp, &mut dpih);
    debug!("Local Domain Participant IH = {:?}", dpih);

    #[allow(clippy::uninit_assumed_init)]
    let mut si: [dds_sample_info_t; MAX_SAMPLES] = { MaybeUninit::uninit().assume_init() };
    let mut samples: [*mut ::std::os::raw::c_void; MAX_SAMPLES] =
        [std::ptr::null_mut(); MAX_SAMPLES as usize];
    samples[0] = std::ptr::null_mut();

    let n = dds_take(
        dr,
        samples.as_mut_ptr() as *mut *mut raw::c_void,
        si.as_mut_ptr() as *mut dds_sample_info_t,
        MAX_SAMPLES as u64,
        MAX_SAMPLES as u32,
    );
    for i in 0..n {
        if si[i as usize].valid_data {
            let sample = samples[i as usize] as *mut dds_builtintopic_endpoint_t;
            debug!(
                "Discovery data from Participant with IH = {:?}",
                (*sample).participant_instance_handle
            );
            let keyless = (*sample).key.v[15] == 3 || (*sample).key.v[15] == 4;
            debug!("Discovered endpoint is keyless: {}", keyless);
            let topic_name = CStr::from_ptr((*sample).topic_name).to_str().unwrap();
            if topic_name.contains("DCPS") || (*sample).participant_instance_handle == dpih {
                debug!("Ignoring discovery from local participant: {}", topic_name);
                // print_qos_partitions((*sample).qos);
                continue;
            }
            let type_name = CStr::from_ptr((*sample).type_name).to_str().unwrap();
            let mut n = 0u32;
            let mut ps: *mut *mut ::std::os::raw::c_char = std::ptr::null_mut();
            let qos = dds_create_qos();
            dds_copy_qos(qos, (*sample).qos);
            dds_qset_ignorelocal(qos, dds_ignorelocal_kind_DDS_IGNORELOCAL_PARTICIPANT);
            dds_qset_history(qos, dds_history_kind_DDS_HISTORY_KEEP_ALL, 0);
            let _ = dds_qget_partition(
                (*sample).qos,
                &mut n as *mut u32,
                &mut ps as *mut *mut *mut ::std::os::raw::c_char,
            );
            if n > 0 {
                for k in 0..n {
                    let p = CStr::from_ptr(*(ps.offset(k as isize))).to_str().unwrap();
                    if si[i as usize].instance_state == dds_instance_state_DDS_IST_ALIVE {
                        if btx.0 {
                            (btx.1)
                                .send(MatchedEntity::DiscoveredPublication {
                                    topic_name: String::from(topic_name),
                                    type_name: String::from(type_name),
                                    keyless,
                                    partition: Some(String::from(p)),
                                    qos: QosHolder(qos),
                                })
                                .unwrap();
                        } else {
                            (btx.1)
                                .send(MatchedEntity::DiscoveredSubscription {
                                    topic_name: String::from(topic_name),
                                    type_name: String::from(type_name),
                                    keyless,
                                    partition: Some(String::from(p)),
                                    qos: QosHolder(qos),
                                })
                                .unwrap();
                        }
                    } else if btx.0 {
                        (btx.1)
                            .send(MatchedEntity::UndiscoveredPublication {
                                topic_name: String::from(topic_name),
                                type_name: String::from(type_name),
                                partition: Some(String::from(p)),
                            })
                            .unwrap();
                    } else {
                        (btx.1)
                            .send(MatchedEntity::UndiscoveredSubscription {
                                topic_name: String::from(topic_name),
                                type_name: String::from(type_name),
                                partition: Some(String::from(p)),
                            })
                            .unwrap();
                    }
                }
            } else if si[i as usize].instance_state == dds_instance_state_DDS_IST_ALIVE {
                if btx.0 {
                    (btx.1)
                        .send(MatchedEntity::DiscoveredPublication {
                            topic_name: String::from(topic_name),
                            type_name: String::from(type_name),
                            keyless,
                            partition: None,
                            qos: QosHolder(qos),
                        })
                        .unwrap();
                } else {
                    (btx.1)
                        .send(MatchedEntity::DiscoveredSubscription {
                            topic_name: String::from(topic_name),
                            type_name: String::from(type_name),
                            keyless,
                            partition: None,
                            qos: QosHolder(qos),
                        })
                        .unwrap();
                }
            } else if btx.0 {
                (btx.1)
                    .send(MatchedEntity::UndiscoveredPublication {
                        topic_name: String::from(topic_name),
                        type_name: String::from(type_name),
                        partition: None,
                    })
                    .unwrap();
            } else {
                (btx.1)
                    .send(MatchedEntity::UndiscoveredSubscription {
                        topic_name: String::from(topic_name),
                        type_name: String::from(type_name),
                        partition: None,
                    })
                    .unwrap();
            }
        }
    }
    dds_return_loan(
        dr,
        samples.as_mut_ptr() as *mut *mut raw::c_void,
        MAX_SAMPLES as i32,
    );
    Box::into_raw(btx);
}
pub fn run_discovery(dp: dds_entity_t, tx: Sender<MatchedEntity>) {
    unsafe {
        let ptx = Box::new((true, tx.clone()));
        let stx = Box::new((false, tx));
        let sub_listener = dds_create_listener(Box::into_raw(ptx) as *mut std::os::raw::c_void);
        dds_lset_data_available(sub_listener, Some(on_data));

        let _pr = dds_create_reader(
            dp,
            DDS_BUILTIN_TOPIC_DCPSPUBLICATION,
            std::ptr::null(),
            sub_listener,
        );

        let sub_listener = dds_create_listener(Box::into_raw(stx) as *mut std::os::raw::c_void);
        dds_lset_data_available(sub_listener, Some(on_data));
        let _sr = dds_create_reader(
            dp,
            DDS_BUILTIN_TOPIC_DCPSSUBSCRIPTION,
            std::ptr::null(),
            sub_listener,
        );
    }
}

unsafe extern "C" fn data_forwarder_listener(dr: dds_entity_t, arg: *mut std::os::raw::c_void) {
    let pa = arg as *mut (ResKey, Arc<Session>, &Box<dyn Coder>);
    let mut zp: *mut cdds_ddsi_payload = std::ptr::null_mut();
    #[allow(clippy::uninit_assumed_init)]
    let mut si: [dds_sample_info_t; 1] = { MaybeUninit::uninit().assume_init() };
    while cdds_take_blob(dr, &mut zp, si.as_mut_ptr()) > 0 {
        if si[0].valid_data {
            log::trace!("Route data to zenoh resource with rid={}", &(*pa).0);
            let bs = Vec::from_raw_parts((*zp).payload, (*zp).size as usize, (*zp).size as usize);
            (*pa).2.encode(bs);
            (*zp).payload = std::ptr::null_mut();
        }
        cdds_serdata_unref(zp as *mut ddsi_serdata);
    }
}
pub fn create_forwarding_dds_reader(
    dp: dds_entity_t,
    topic_name: String,
    type_name: String,
    keyless: bool,
    qos: QosHolder,
    z_key: ResKey,
    z: Arc<Session>,
    coders: &Coders,
) -> dds_entity_t {
    let writer = ZenohWriter::new(z.clone(), z_key.clone());
    let encoder: Box<dyn Coder> = coders.new_encoder(&topic_name, &type_name, Box::new(writer));
    let cton = CString::new(topic_name).unwrap().into_raw();
    let ctyn = CString::new(type_name).unwrap().into_raw();

    unsafe {
        let t = cdds_create_blob_topic(dp, cton, ctyn, keyless);
        let arg = Box::new((z_key, z, Box::into_raw(Box::new(encoder))));
        let sub_listener = dds_create_listener(Box::into_raw(arg) as *mut std::os::raw::c_void);
        dds_lset_data_available(sub_listener, Some(data_forwarder_listener));
        dds_create_reader(dp, t, qos.0, sub_listener)
    }
}

pub fn create_forwarding_dds_writer(
    dp: dds_entity_t,
    topic_name: String,
    type_name: String,
    keyless: bool,
    qos: QosHolder,
) -> dds_entity_t {
    let cton = CString::new(topic_name).unwrap().into_raw();
    let ctyn = CString::new(type_name).unwrap().into_raw();

    unsafe {
        let t = cdds_create_blob_topic(dp, cton, ctyn, keyless);
        dds_create_writer(dp, t, qos.0, std::ptr::null_mut())
    }
}
